package app.organicmaps.dialog;

import android.app.Dialog;
import android.location.Location;
import android.os.Bundle;
import android.widget.Toast;
import androidx.annotation.NonNull;
import androidx.annotation.Nullable;
import androidx.fragment.app.DialogFragment;
import androidx.fragment.app.FragmentManager;
import app.organicmaps.MwmApplication;
import app.organicmaps.R;
import app.organicmaps.sdk.hazards.Hazard;
import app.organicmaps.sdk.hazards.HazardDraft;
import app.organicmaps.sdk.hazards.HazardReporter;
import app.organicmaps.sdk.hazards.HazardType;
import com.google.android.material.dialog.MaterialAlertDialogBuilder;

/** Picks a hazard type and reports it at the last known position. */
public class HazardReportDialogFragment extends DialogFragment
{
  private static final String TAG = HazardReportDialogFragment.class.getSimpleName();

  private static final HazardType[] TYPES = {
      HazardType.POTHOLE, HazardType.ACCIDENT, HazardType.ROAD_CLOSURE, HazardType.CONSTRUCTION,
      HazardType.STOPPED_VEHICLE, HazardType.FLOODING, HazardType.DEBRIS, HazardType.ANIMAL,
  };

  public static void show(@NonNull FragmentManager manager)
  {
    new HazardReportDialogFragment().show(manager, TAG);
  }

  @NonNull
  @Override
  public Dialog onCreateDialog(@Nullable Bundle savedInstanceState)
  {
    final int[] labels = {
        R.string.hazard_type_pothole, R.string.hazard_type_accident, R.string.hazard_type_closure,
        R.string.hazard_type_construction, R.string.hazard_type_stopped, R.string.hazard_type_flooding,
        R.string.hazard_type_debris, R.string.hazard_type_animal,
    };
    final CharSequence[] items = new CharSequence[labels.length];
    for (int i = 0; i < labels.length; i++)
      items[i] = getString(labels[i]);

    return new MaterialAlertDialogBuilder(requireContext())
        .setTitle(R.string.hazard_report_title)
        .setItems(items, (dialog, which) -> report(TYPES[which]))
        .setNegativeButton(android.R.string.cancel, null)
        .create();
  }

  private void report(@NonNull HazardType type)
  {
    final android.content.Context context = requireContext().getApplicationContext();
    if (!HazardReporter.isConfigured(context))
    {
      Toast.makeText(requireContext(), R.string.hazard_not_configured, Toast.LENGTH_LONG).show();
      return;
    }

    final Location location = MwmApplication.from(context).getLocationHelper().getSavedLocation();
    if (location == null)
    {
      Toast.makeText(requireContext(), R.string.hazard_no_location, Toast.LENGTH_LONG).show();
      return;
    }

    final HazardDraft draft =
        new HazardDraft(type, location.getLongitude(), location.getLatitude(), null);
    HazardReporter.report(context, draft, new HazardReporter.Callback()
    {
      @Override
      public void onSuccess(@NonNull Hazard hazard)
      {
        Toast.makeText(requireContext(), R.string.hazard_report_sent, Toast.LENGTH_SHORT).show();
      }

      @Override
      public void onError(@NonNull String message)
      {
        Toast.makeText(requireContext(), getString(R.string.hazard_report_failed, message),
                       Toast.LENGTH_LONG).show();
      }
    });
  }
}
