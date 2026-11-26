package app.organicmaps.hazard;

import android.app.Activity;
import android.content.Context;
import android.widget.Toast;
import androidx.annotation.NonNull;
import androidx.annotation.Nullable;
import app.organicmaps.R;
import app.organicmaps.sdk.Framework;
import app.organicmaps.sdk.hazards.Hazard;
import app.organicmaps.sdk.hazards.HazardReporter;
import app.organicmaps.sdk.hazards.HazardType;
import com.google.android.material.dialog.MaterialAlertDialogBuilder;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.List;
import java.util.Locale;

/** Lists hazards around the viewport, centred on the map, and lets the user vote. */
public final class NearbyHazards
{
  private static final double BBOX_DEG = 0.05;
  private static final int ZOOM = 16;

  private NearbyHazards()
  {
  }

  public static void show(@NonNull Activity activity)
  {
    final Context context = activity.getApplicationContext();
    if (!HazardReporter.isConfigured(context))
    {
      toast(activity, R.string.hazard_not_configured);
      return;
    }
    final double[] center = Framework.nativeGetScreenRectCenter();
    if (center == null)
    {
      toast(activity, R.string.hazard_no_location);
      return;
    }
    final double lat = center[0];
    final double lon = center[1];

    HazardReporter.list(context, lon - BBOX_DEG, lat - BBOX_DEG, lon + BBOX_DEG, lat + BBOX_DEG,
                        new HazardReporter.ListCallback()
    {
      @Override
      public void onSuccess(@NonNull List<Hazard> hazards)
      {
        present(activity, hazards, lat, lon);
      }

      @Override
      public void onError(@NonNull String message)
      {
        toast(activity, R.string.hazard_report_failed, message);
      }
    });
  }

  private static void present(@NonNull Activity activity, @NonNull List<Hazard> hazards,
                              double centerLat, double centerLon)
  {
    if (activity.isFinishing() || activity.isDestroyed())
      return;
    if (hazards.isEmpty())
    {
      toast(activity, R.string.no_hazards_nearby);
      return;
    }

    final List<Hazard> sorted = new ArrayList<>(hazards);
    sorted.sort(Comparator.comparingDouble(h -> distanceM(centerLat, centerLon, h.lat, h.lon)));

    final CharSequence[] items = new CharSequence[sorted.size()];
    for (int i = 0; i < sorted.size(); i++)
    {
      final Hazard hazard = sorted.get(i);
      final double km = distanceM(centerLat, centerLon, hazard.lat, hazard.lon) / 1000.0;
      items[i] = activity.getString(typeLabel(hazard.type)) + " · "
          + String.format(Locale.getDefault(), "%.1f km", km);
    }

    new MaterialAlertDialogBuilder(activity)
        .setTitle(R.string.nearby_hazards_title)
        .setItems(items, (dialog, which) -> {
          final Hazard hazard = sorted.get(which);
          Framework.nativeSetViewportCenter(hazard.lat, hazard.lon, ZOOM);
          showVote(activity, hazard);
        })
        .setNegativeButton(android.R.string.cancel, null)
        .show();
  }

  public static void showVote(@NonNull Activity activity, @NonNull Hazard hazard)
  {
    new MaterialAlertDialogBuilder(activity)
        .setTitle(typeLabel(hazard.type))
        .setMessage(R.string.hazard_vote_question)
        .setPositiveButton(R.string.hazard_vote_confirm,
                           (d, w) -> vote(activity, hazard, "confirm"))
        .setNeutralButton(R.string.hazard_vote_deny, (d, w) -> vote(activity, hazard, "deny"))
        .setNegativeButton(android.R.string.cancel, null)
        .show();
  }

  private static void vote(@NonNull Activity activity, @NonNull Hazard hazard, @NonNull String vote)
  {
    HazardReporter.vote(activity, hazard.id, vote, new HazardReporter.Callback()
    {
      @Override
      public void onSuccess(@NonNull Hazard updated)
      {
        toast(activity, R.string.hazard_vote_sent);
      }

      @Override
      public void onError(@NonNull String message)
      {
        toast(activity, R.string.hazard_report_failed, message);
      }
    });
  }

  private static int typeLabel(@Nullable HazardType type)
  {
    if (type == null)
      return R.string.hazard_type_debris;
    switch (type)
    {
      case ACCIDENT: return R.string.hazard_type_accident;
      case POTHOLE: return R.string.hazard_type_pothole;
      case ROAD_CLOSURE: return R.string.hazard_type_closure;
      case CONSTRUCTION: return R.string.hazard_type_construction;
      case STOPPED_VEHICLE: return R.string.hazard_type_stopped;
      case FLOODING: return R.string.hazard_type_flooding;
      case ANIMAL: return R.string.hazard_type_animal;
      case DEBRIS:
      case HAZARD:
      default: return R.string.hazard_type_debris;
    }
  }

  private static void toast(@NonNull Activity activity, int resId)
  {
    if (activity.isFinishing() || activity.isDestroyed())
      return;
    Toast.makeText(activity, resId, Toast.LENGTH_SHORT).show();
  }

  private static void toast(@NonNull Activity activity, int resId, @Nullable String detail)
  {
    if (activity.isFinishing() || activity.isDestroyed())
      return;
    Toast.makeText(activity, activity.getString(resId, detail), Toast.LENGTH_LONG).show();
  }

  private static double distanceM(double lat1, double lon1, double lat2, double lon2)
  {
    final double r = 6_371_000.0;
    final double dLat = Math.toRadians(lat2 - lat1);
    final double dLon = Math.toRadians(lon2 - lon1);
    final double a = Math.sin(dLat / 2) * Math.sin(dLat / 2)
        + Math.cos(Math.toRadians(lat1)) * Math.cos(Math.toRadians(lat2))
            * Math.sin(dLon / 2) * Math.sin(dLon / 2);
    return 2 * r * Math.asin(Math.sqrt(a));
  }
}
