package app.organicmaps.sdk.hazards;

import androidx.annotation.NonNull;
import androidx.annotation.Nullable;
import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;

/** A hazard as returned by the service. */
public final class Hazard
{
  @NonNull
  public final String id;
  @Nullable
  public final HazardType type;
  public final double lon;
  public final double lat;
  @NonNull
  public final String reportedAt;
  @NonNull
  public final String expiresAt;
  @NonNull
  public final String status;
  public final double confidence;
  public final int confirmations;
  public final boolean deleted;

  private Hazard(@NonNull String id, @Nullable HazardType type, double lon, double lat,
                 @NonNull String reportedAt, @NonNull String expiresAt, @NonNull String status,
                 double confidence, int confirmations, boolean deleted)
  {
    this.id = id;
    this.type = type;
    this.lon = lon;
    this.lat = lat;
    this.reportedAt = reportedAt;
    this.expiresAt = expiresAt;
    this.status = status;
    this.confidence = confidence;
    this.confirmations = confirmations;
    this.deleted = deleted;
  }

  @NonNull
  static Hazard fromJson(@NonNull JSONObject object) throws JSONException
  {
    final JSONObject geom = object.getJSONObject("geom");
    final JSONArray coordinates = geom.getJSONArray("coordinates");
    return new Hazard(object.getString("id"), HazardType.fromApi(object.optString("type", null)),
                      coordinates.getDouble(0), coordinates.getDouble(1),
                      object.optString("reported_at", ""), object.optString("expires_at", ""),
                      object.optString("status", "active"), object.optDouble("confidence", 0.0),
                      object.optInt("confirmations", 0), object.optBoolean("deleted", false));
  }
}
