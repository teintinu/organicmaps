package app.organicmaps.sdk.hazards;

import androidx.annotation.NonNull;
import androidx.annotation.Nullable;
import java.io.IOException;
import java.util.ArrayList;
import java.util.List;
import java.util.Locale;
import okhttp3.HttpUrl;
import okhttp3.MediaType;
import okhttp3.OkHttpClient;
import okhttp3.Request;
import okhttp3.RequestBody;
import okhttp3.Response;
import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;

/**
 * Client for the self-hostable hazard API v1.
 *
 * <p>Methods are synchronous and must be called off the UI thread; the API surface is deliberately
 * small so the map layer can wrap it in its own executor.
 */
public final class HazardClient
{
  private static final MediaType JSON = MediaType.get("application/json; charset=utf-8");

  @NonNull
  private final OkHttpClient mHttp;
  @NonNull
  private final String mBaseUrl;
  @Nullable
  private String mToken;

  public HazardClient(@NonNull String baseUrl)
  {
    this(baseUrl, new OkHttpClient());
  }

  public HazardClient(@NonNull String baseUrl, @NonNull OkHttpClient http)
  {
    mBaseUrl = baseUrl.endsWith("/") ? baseUrl.substring(0, baseUrl.length() - 1) : baseUrl;
    mHttp = http;
  }

  public void setToken(@Nullable String token)
  {
    mToken = token;
  }

  @Nullable
  public String getToken()
  {
    return mToken;
  }

  @NonNull
  public DeviceToken registerDevice() throws IOException
  {
    final Request request = new Request.Builder()
                                .url(mBaseUrl + "/v1/devices")
                                .post(RequestBody.create("{}", JSON))
                                .build();
    try
    {
      return DeviceToken.fromJson(execute(request));
    }
    catch (JSONException e)
    {
      throw new IOException("Malformed device response", e);
    }
  }

  @NonNull
  public Hazard report(@NonNull HazardDraft draft, @NonNull String idempotencyKey) throws IOException
  {
    final String body;
    try
    {
      body = draft.toJson().toString();
    }
    catch (JSONException e)
    {
      throw new IOException("Cannot encode report", e);
    }
    final Request request = authorized(mBaseUrl + "/v1/hazards")
                                .header("Idempotency-Key", idempotencyKey)
                                .post(RequestBody.create(body, JSON))
                                .build();
    try
    {
      return Hazard.fromJson(execute(request));
    }
    catch (JSONException e)
    {
      throw new IOException("Malformed hazard response", e);
    }
  }

  /** Returns hazards in the bbox; a non-null {@code sinceMs} asks for a delta. */
  @NonNull
  public List<Hazard> list(double minLon, double minLat, double maxLon, double maxLat,
                           @Nullable Long sinceMs, int limit) throws IOException
  {
    final HttpUrl.Builder url = HttpUrl.get(mBaseUrl + "/v1/hazards").newBuilder();
    url.addQueryParameter("bbox", String.format(Locale.US, "%f,%f,%f,%f", minLon, minLat, maxLon, maxLat));
    url.addQueryParameter("limit", Integer.toString(limit));
    if (sinceMs != null)
      url.addQueryParameter("since", Long.toString(sinceMs));

    final Request request = authorized(url.build().toString()).get().build();
    final JSONObject body = execute(request);
    try
    {
      final JSONArray features = body.getJSONArray("features");
      final List<Hazard> hazards = new ArrayList<>(features.length());
      for (int i = 0; i < features.length(); i++)
      {
        final JSONObject properties = features.getJSONObject(i).getJSONObject("properties");
        hazards.add(Hazard.fromJson(properties));
      }
      return hazards;
    }
    catch (JSONException e)
    {
      throw new IOException("Malformed hazard collection", e);
    }
  }

  @NonNull
  public Hazard vote(@NonNull String hazardId, @NonNull String vote) throws IOException
  {
    final JSONObject body = new JSONObject();
    try
    {
      body.put("vote", vote);
    }
    catch (JSONException e)
    {
      throw new IOException("Cannot encode vote", e);
    }
    final Request request = authorized(mBaseUrl + "/v1/hazards/" + hazardId + "/votes")
                                .post(RequestBody.create(body.toString(), JSON))
                                .build();
    try
    {
      return Hazard.fromJson(execute(request));
    }
    catch (JSONException e)
    {
      throw new IOException("Malformed hazard response", e);
    }
  }

  /** LGPD/GDPR erasure: removes the device and anonymizes its reports. */
  public void deleteMe() throws IOException
  {
    final Request request = authorized(mBaseUrl + "/v1/me").delete().build();
    try (Response response = mHttp.newCall(request).execute())
    {
      if (!response.isSuccessful())
        throw new IOException(errorMessage(response));
    }
  }

  @NonNull
  private Request.Builder authorized(@NonNull String url)
  {
    final Request.Builder builder = new Request.Builder().url(url);
    if (mToken != null)
      builder.header("Authorization", "Bearer " + mToken);
    return builder;
  }

  @NonNull
  private JSONObject execute(@NonNull Request request) throws IOException
  {
    try (Response response = mHttp.newCall(request).execute())
    {
      if (!response.isSuccessful())
        throw new IOException(errorMessage(response));
      if (response.body() == null)
        return new JSONObject();
      final String text = response.body().string();
      return text.isEmpty() ? new JSONObject() : new JSONObject(text);
    }
    catch (JSONException e)
    {
      throw new IOException("Malformed JSON response", e);
    }
  }

  @NonNull
  private static String errorMessage(@NonNull Response response) throws IOException
  {
    String detail = "";
    if (response.body() != null)
    {
      try
      {
        final JSONObject problem = new JSONObject(response.body().string());
        detail = problem.optString("title", "");
        final String extra = problem.optString("detail", "");
        if (!extra.isEmpty() && !"null".equals(extra))
          detail += ": " + extra;
      }
      catch (JSONException ignored)
      {
      }
    }
    return "HTTP " + response.code() + (detail.isEmpty() ? "" : " (" + detail + ")");
  }
}
