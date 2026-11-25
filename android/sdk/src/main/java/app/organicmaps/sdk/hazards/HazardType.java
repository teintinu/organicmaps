package app.organicmaps.sdk.hazards;

import androidx.annotation.NonNull;
import androidx.annotation.Nullable;

/**
 * Hazard kinds accepted by the service (API v1).
 *
 * <p>There is deliberately no POLICE type: highest abuse rate, legal exposure in several countries,
 * and no navigation value (decision D8).
 */
public enum HazardType
{
  ACCIDENT("accident"),
  HAZARD("hazard"),
  POTHOLE("pothole"),
  ROAD_CLOSURE("road_closure"),
  CONSTRUCTION("construction"),
  STOPPED_VEHICLE("stopped_vehicle"),
  FLOODING("flooding"),
  DEBRIS("debris"),
  ANIMAL("animal");

  @NonNull
  private final String mApiValue;

  HazardType(@NonNull String apiValue)
  {
    mApiValue = apiValue;
  }

  @NonNull
  public String apiValue()
  {
    return mApiValue;
  }

  @Nullable
  public static HazardType fromApi(@Nullable String value)
  {
    if (value == null)
      return null;
    for (HazardType type : values())
    {
      if (type.mApiValue.equals(value))
        return type;
    }
    return null;
  }
}
