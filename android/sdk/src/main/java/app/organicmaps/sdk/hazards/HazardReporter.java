package app.organicmaps.sdk.hazards;

import android.content.Context;
import android.os.Handler;
import android.os.Looper;
import androidx.annotation.NonNull;
import java.io.IOException;
import java.util.List;
import java.util.UUID;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import okhttp3.OkHttpClient;

/** Sends hazard reports off the UI thread using the stored endpoint and token. */
public final class HazardReporter
{
  public interface Callback
  {
    void onSuccess(@NonNull Hazard hazard);

    void onError(@NonNull String message);
  }

  public interface ListCallback
  {
    void onSuccess(@NonNull List<Hazard> hazards);

    void onError(@NonNull String message);
  }

  private static final ExecutorService EXECUTOR = Executors.newSingleThreadExecutor();
  private static final Handler MAIN = new Handler(Looper.getMainLooper());

  private HazardReporter()
  {
  }

  public static boolean isConfigured(@NonNull Context context)
  {
    return HazardSettings.getBaseUrl(context) != null;
  }

  @NonNull
  private static HazardClient clientWithToken(@NonNull Context context) throws IOException
  {
    final String baseUrl = HazardSettings.getBaseUrl(context);
    if (baseUrl == null)
      throw new IOException("no hazard service configured");
    final HazardClient client = new HazardClient(baseUrl, new OkHttpClient());
    String token = HazardSettings.getToken(context);
    if (token == null)
    {
      token = client.registerDevice().token;
      HazardSettings.setToken(context, token);
    }
    client.setToken(token);
    return client;
  }

  public static void list(@NonNull Context context, double minLon, double minLat, double maxLon,
                          double maxLat, @NonNull ListCallback callback)
  {
    final Context appContext = context.getApplicationContext();
    EXECUTOR.execute(() -> {
      try
      {
        final List<Hazard> hazards = clientWithToken(appContext).list(minLon, minLat, maxLon, maxLat, null, 100);
        MAIN.post(() -> callback.onSuccess(hazards));
      }
      catch (IOException e)
      {
        MAIN.post(() -> callback.onError(e.getMessage() == null ? e.toString() : e.getMessage()));
      }
    });
  }

  public static void vote(@NonNull Context context, @NonNull String hazardId,
                          @NonNull String vote, @NonNull Callback callback)
  {
    final Context appContext = context.getApplicationContext();
    EXECUTOR.execute(() -> {
      try
      {
        final Hazard hazard = clientWithToken(appContext).vote(hazardId, vote);
        MAIN.post(() -> callback.onSuccess(hazard));
      }
      catch (IOException e)
      {
        postError(callback, e.getMessage() == null ? e.toString() : e.getMessage());
      }
    });
  }

  public static void report(@NonNull Context context, @NonNull HazardDraft draft,
                            @NonNull Callback callback)
  {
    final Context appContext = context.getApplicationContext();
    EXECUTOR.execute(() -> {
      try
      {
        final HazardClient client = clientWithToken(appContext);
        final Hazard hazard = client.report(draft, UUID.randomUUID().toString());
        MAIN.post(() -> callback.onSuccess(hazard));
      }
      catch (IOException e)
      {
        postError(callback, e.getMessage() == null ? e.toString() : e.getMessage());
      }
    });
  }

  private static void postError(@NonNull Callback callback, @NonNull String message)
  {
    MAIN.post(() -> callback.onError(message));
  }
}
