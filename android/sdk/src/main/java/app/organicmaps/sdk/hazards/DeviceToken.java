package app.organicmaps.sdk.hazards;

import androidx.annotation.NonNull;
import org.json.JSONException;
import org.json.JSONObject;

/** Anonymous device credentials returned by {@code POST /v1/devices}. */
public final class DeviceToken
{
  @NonNull
  public final String token;
  @NonNull
  public final String expiresAt;

  DeviceToken(@NonNull String token, @NonNull String expiresAt)
  {
    this.token = token;
    this.expiresAt = expiresAt;
  }

  @NonNull
  static DeviceToken fromJson(@NonNull JSONObject object) throws JSONException
  {
    return new DeviceToken(object.getString("token"), object.optString("expiresAt", ""));
  }
}
