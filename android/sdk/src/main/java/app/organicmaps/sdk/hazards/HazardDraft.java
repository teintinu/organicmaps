package app.organicmaps.sdk.hazards;

import androidx.annotation.NonNull;
import androidx.annotation.Nullable;
import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;

/** A report the user wants to publish. */
public final class HazardDraft
{
  @NonNull
  private final HazardType mType;
  private final double mLon;
  private final double mLat;
  @Nullable
  private final String mSubtype;

  public HazardDraft(@NonNull HazardType type, double lon, double lat, @Nullable String subtype)
  {
    mType = type;
    mLon = lon;
    mLat = lat;
    mSubtype = subtype;
  }

  @NonNull
  public HazardType getType()
  {
    return mType;
  }

  public double getLon()
  {
    return mLon;
  }

  public double getLat()
  {
    return mLat;
  }

  @NonNull
  JSONObject toJson() throws JSONException
  {
    final JSONObject geom = new JSONObject();
    geom.put("type", "Point");
    geom.put("coordinates", new JSONArray().put(mLon).put(mLat));

    final JSONObject body = new JSONObject();
    body.put("type", mType.apiValue());
    body.put("geom", geom);
    if (mSubtype != null)
      body.put("subtype", mSubtype);
    return body;
  }
}
