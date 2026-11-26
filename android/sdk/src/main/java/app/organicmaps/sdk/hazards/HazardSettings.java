package app.organicmaps.sdk.hazards;

import android.content.Context;
import android.content.SharedPreferences;
import androidx.annotation.NonNull;
import androidx.annotation.Nullable;

/** Local storage for the hazard endpoint and the anonymous device token. */
public final class HazardSettings
{
  private static final String PREFS = "hazard_prefs";
  private static final String KEY_BASE_URL = "base_url";
  private static final String KEY_TOKEN = "device_token";

  private HazardSettings()
  {
  }

  @NonNull
  private static SharedPreferences prefs(@NonNull Context context)
  {
    return context.getApplicationContext().getSharedPreferences(PREFS, Context.MODE_PRIVATE);
  }

  @Nullable
  public static String getBaseUrl(@NonNull Context context)
  {
    final String value = prefs(context).getString(KEY_BASE_URL, null);
    return value == null || value.isEmpty() ? null : value;
  }

  public static void setBaseUrl(@NonNull Context context, @Nullable String url)
  {
    prefs(context).edit().putString(KEY_BASE_URL, url == null ? "" : url.trim()).apply();
  }

  /** Fills the endpoint on first launch without overriding a user choice. */
  public static void ensureDefaultBaseUrl(@NonNull Context context, @Nullable String url)
  {
    if (url == null || url.isEmpty() || getBaseUrl(context) != null)
      return;
    setBaseUrl(context, url);
  }

  @Nullable
  public static String getToken(@NonNull Context context)
  {
    return prefs(context).getString(KEY_TOKEN, null);
  }

  public static void setToken(@NonNull Context context, @Nullable String token)
  {
    prefs(context).edit().putString(KEY_TOKEN, token).apply();
  }
}
