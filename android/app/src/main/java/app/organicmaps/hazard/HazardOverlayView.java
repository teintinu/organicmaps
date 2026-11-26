package app.organicmaps.hazard;

import android.content.Context;
import android.graphics.Canvas;
import android.graphics.Color;
import android.graphics.Paint;
import android.os.Handler;
import android.os.Looper;
import android.util.AttributeSet;
import android.view.MotionEvent;
import android.view.View;
import androidx.annotation.NonNull;
import androidx.annotation.Nullable;
import app.organicmaps.sdk.Framework;
import app.organicmaps.sdk.hazards.Hazard;
import app.organicmaps.sdk.hazards.HazardReporter;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;

/** Draws reported hazards over the map and turns taps into a vote. */
public class HazardOverlayView extends View
{
  public interface Listener
  {
    void onHazardTap(@NonNull Hazard hazard);
  }

  private static final double BBOX_DEG = 0.05;
  private static final long REFRESH_MS = 15_000;

  private final Paint mFill = new Paint(Paint.ANTI_ALIAS_FLAG);
  private final Paint mStroke = new Paint(Paint.ANTI_ALIAS_FLAG);
  private final float mRadiusPx;
  private final float mTouchRadiusPx;
  private final Handler mHandler = new Handler(Looper.getMainLooper());
  private final Runnable mRefreshTask = this::refresh;

  @NonNull
  private List<Hazard> mHazards = Collections.emptyList();
  @Nullable
  private Listener mListener;
  @Nullable
  private Hazard mPressed;

  public HazardOverlayView(@NonNull Context context, @Nullable AttributeSet attrs)
  {
    super(context, attrs);
    final float density = getResources().getDisplayMetrics().density;
    mRadiusPx = 6f * density;
    mTouchRadiusPx = 28f * density;
    mFill.setColor(Color.rgb(0xF5, 0x7C, 0x00));
    mStroke.setColor(Color.WHITE);
    mStroke.setStyle(Paint.Style.STROKE);
    mStroke.setStrokeWidth(1.5f * density);
    setWillNotDraw(false);
  }

  public void setListener(@Nullable Listener listener)
  {
    mListener = listener;
  }

  public void startRefreshing(@NonNull Context context)
  {
    mHandler.removeCallbacks(mRefreshTask);
    mHandler.post(mRefreshTask);
  }

  public void stopRefreshing()
  {
    mHandler.removeCallbacks(mRefreshTask);
    mHazards = Collections.emptyList();
    invalidate();
  }

  private void refresh()
  {
    final double[] center = Framework.nativeGetParsedCenterLatLon();
    if (center == null)
    {
      mHandler.postDelayed(mRefreshTask, REFRESH_MS);
      return;
    }
    final double lat = center[0];
    final double lon = center[1];
    HazardReporter.list(getContext(), lon - BBOX_DEG, lat - BBOX_DEG, lon + BBOX_DEG, lat + BBOX_DEG,
                        new HazardReporter.ListCallback()
    {
      @Override
      public void onSuccess(@NonNull List<Hazard> hazards)
      {
        mHazards = new ArrayList<>(hazards);
        invalidate();
        mHandler.postDelayed(mRefreshTask, REFRESH_MS);
      }

      @Override
      public void onError(@NonNull String message)
      {
        mHandler.postDelayed(mRefreshTask, REFRESH_MS);
      }
    });
  }

  @Override
  protected void onDraw(@NonNull Canvas canvas)
  {
    super.onDraw(canvas);
    for (Hazard hazard : mHazards)
    {
      final double[] pixel = Framework.nativeToScreen(hazard.lat, hazard.lon);
      if (pixel == null || pixel.length < 3 || pixel[2] < 0.5)
        continue;
      canvas.drawCircle((float) pixel[0], (float) pixel[1], mRadiusPx, mFill);
      canvas.drawCircle((float) pixel[0], (float) pixel[1], mRadiusPx, mStroke);
    }
  }

  @Override
  public boolean onTouchEvent(@NonNull MotionEvent event)
  {
    if (event.getAction() == MotionEvent.ACTION_DOWN)
    {
      mPressed = findHazard(event.getX(), event.getY());
      return mPressed != null;
    }
    if (event.getAction() == MotionEvent.ACTION_UP)
    {
      final Hazard hazard = mPressed;
      mPressed = null;
      if (hazard != null && mListener != null)
      {
        mListener.onHazardTap(hazard);
        return true;
      }
    }
    return false;
  }

  @Nullable
  private Hazard findHazard(float x, float y)
  {
    Hazard best = null;
    double bestDistance = mTouchRadiusPx;
    for (Hazard hazard : mHazards)
    {
      final double[] pixel = Framework.nativeToScreen(hazard.lat, hazard.lon);
      if (pixel == null || pixel.length < 2)
        continue;
      final double dx = pixel[0] - x;
      final double dy = pixel[1] - y;
      final double distance = Math.sqrt(dx * dx + dy * dy);
      if (distance < bestDistance)
      {
        bestDistance = distance;
        best = hazard;
      }
    }
    return best;
  }
}
