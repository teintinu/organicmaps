package app.organicmaps.sdk.hazards;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertTrue;
import static org.junit.Assert.fail;

import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.InetAddress;
import java.net.ServerSocket;
import java.net.Socket;
import java.nio.charset.StandardCharsets;
import java.util.HashMap;
import java.util.Locale;
import java.util.Map;
import java.util.List;
import java.util.concurrent.atomic.AtomicReference;
import org.junit.After;
import org.junit.Before;
import org.junit.Test;

public class HazardClientTest
{
  private ServerSocket mServer;
  private Thread mServerThread;
  private String mBaseUrl;
  private int mReplyCode;
  private String mReplyBody;
  private final AtomicReference<String> mMethod = new AtomicReference<>();
  private final AtomicReference<String> mPath = new AtomicReference<>();
  private final AtomicReference<String> mQuery = new AtomicReference<>();
  private final AtomicReference<String> mAuth = new AtomicReference<>();
  private final AtomicReference<String> mIdempotency = new AtomicReference<>();
  private final AtomicReference<String> mBody = new AtomicReference<>();

  @Before
  public void setUp() throws IOException
  {
    mServer = new ServerSocket(0, 50, InetAddress.getByName("127.0.0.1"));
    mServerThread = new Thread(this::serve, "hazard-test-server");
    mServerThread.setDaemon(true);
    mServerThread.start();
    mBaseUrl = "http://127.0.0.1:" + mServer.getLocalPort();
  }

  @After
  public void tearDown() throws IOException
  {
    mServer.close();
  }

  /** Minimal HTTP/1.1 server: one request per connection, no keep-alive. */
  private void serve()
  {
    while (!mServer.isClosed())
    {
      try (Socket socket = mServer.accept())
      {
        handle(socket);
      }
      catch (IOException ignored)
      {
        // Accept loop ends when the socket is closed in tearDown().
      }
    }
  }

  private void handle(Socket socket) throws IOException
  {
    final InputStream in = socket.getInputStream();
    final ByteArrayOutputStream head = new ByteArrayOutputStream();
    long lastFour = 0;
    while (true)
    {
      final int b = in.read();
      if (b < 0)
        return;
      head.write(b);
      lastFour = ((lastFour << 8) | (b & 0xff)) & 0xffffffffL;
      if (lastFour == 0x0d0a0d0aL)
        break;
    }

    final String[] lines = head.toString(StandardCharsets.UTF_8).split("\r\n");
    final String[] requestLine = lines[0].split(" ");
    mMethod.set(requestLine[0]);
    final String[] target = requestLine[1].split("\\?", 2);
    mPath.set(target[0]);
    mQuery.set(target.length > 1 ? target[1] : null);

    final Map<String, String> headers = new HashMap<>();
    for (int i = 1; i < lines.length; i++)
    {
      final int colon = lines[i].indexOf(':');
      if (colon > 0)
        headers.put(lines[i].substring(0, colon).trim().toLowerCase(Locale.ROOT),
                    lines[i].substring(colon + 1).trim());
    }
    mAuth.set(headers.get("authorization"));
    mIdempotency.set(headers.get("idempotency-key"));

    final int length = Integer.parseInt(headers.getOrDefault("content-length", "0"));
    mBody.set(new String(in.readNBytes(length), StandardCharsets.UTF_8));

    final byte[] payload = mReplyBody.getBytes(StandardCharsets.UTF_8);
    final String response = "HTTP/1.1 " + mReplyCode + " X\r\n"
        + "content-type: application/json\r\n"
        + "content-length: " + payload.length + "\r\n"
        + "connection: close\r\n\r\n";
    try (OutputStream out = socket.getOutputStream())
    {
      out.write(response.getBytes(StandardCharsets.UTF_8));
      out.write(payload);
      out.flush();
    }
  }

  private static String hazardJson(String id, String type, double lon, double lat, int confirmations)
  {
    return "{\"id\":\"" + id + "\",\"type\":\"" + type + "\",\"geom\":{\"type\":\"Point\","
        + "\"coordinates\":[" + lon + "," + lat + "]},\"reported_at\":\"2026-11-25T12:00:00Z\","
        + "\"expires_at\":\"2027-02-23T12:00:00Z\",\"status\":\"active\",\"confidence\":0.6,"
        + "\"confirmations\":" + confirmations + ",\"deleted\":false}";
  }

  @Test
  public void registersAnAnonymousDevice() throws IOException
  {
    mReplyCode = 201;
    mReplyBody = "{\"token\":\"tok-123\",\"expiresAt\":\"2027-02-23T12:00:00Z\"}";

    final DeviceToken device = new HazardClient(mBaseUrl).registerDevice();

    assertEquals("tok-123", device.token);
    assertEquals("POST", mMethod.get());
    assertEquals("/v1/devices", mPath.get());
  }

  @Test
  public void reportsWithAuthAndIdempotency() throws IOException
  {
    mReplyCode = 201;
    mReplyBody = hazardJson("h1", "pothole", -46.6333, -23.5505, 0);

    final HazardClient client = new HazardClient(mBaseUrl);
    client.setToken("tok-123");
    final Hazard hazard = client.report(new HazardDraft(HazardType.POTHOLE, -46.6333, -23.5505, null), "key-1");

    assertEquals("h1", hazard.id);
    assertEquals(HazardType.POTHOLE, hazard.type);
    assertEquals(0, hazard.confirmations);
    assertEquals("/v1/hazards", mPath.get());
    assertEquals("Bearer tok-123", mAuth.get());
    assertEquals("key-1", mIdempotency.get());
    assertTrue(mBody.get().contains("\"pothole\""));
  }

  @Test
  public void listsHazardsFromFeatureCollection() throws IOException
  {
    mReplyCode = 200;
    mReplyBody = "{\"type\":\"FeatureCollection\",\"features\":[{\"type\":\"Feature\","
        + "\"geometry\":{\"type\":\"Point\",\"coordinates\":[-46.6,-23.5]},"
        + "\"properties\":" + hazardJson("h2", "flooding", -46.6, -23.5, 1) + "}],"
        + "\"meta\":{\"nextSince\":\"123\",\"serverTime\":\"2026-11-25T12:00:00Z\"}}";

    final List<Hazard> hazards = new HazardClient(mBaseUrl).list(-46.7, -23.6, -46.5, -23.4, 42L, 10);

    assertEquals(1, hazards.size());
    assertEquals("h2", hazards.get(0).id);
    assertEquals(-23.5, hazards.get(0).lat, 1e-9);
    assertTrue(mQuery.get().contains("bbox="));
    assertTrue(mQuery.get().contains("since=42"));
  }

  @Test
  public void votesOnAHazard() throws IOException
  {
    mReplyCode = 200;
    mReplyBody = hazardJson("h1", "accident", -46.6, -23.5, 2);

    final HazardClient client = new HazardClient(mBaseUrl);
    client.setToken("tok-123");
    final Hazard hazard = client.vote("h1", "confirm");

    assertEquals(2, hazard.confirmations);
    assertEquals("/v1/hazards/h1/votes", mPath.get());
    assertTrue(mBody.get().contains("\"confirm\""));
  }

  @Test
  public void surfacesServerErrors()
  {
    mReplyCode = 401;
    mReplyBody = "{\"title\":\"missing or invalid device token\",\"status\":401}";

    try
    {
      new HazardClient(mBaseUrl).report(new HazardDraft(HazardType.DEBRIS, 0.0, 0.0, null), "k");
      fail("expected IOException");
    }
    catch (IOException e)
    {
      assertTrue(e.getMessage(), e.getMessage().contains("401"));
      assertTrue(e.getMessage(), e.getMessage().contains("device token"));
    }
  }
}
