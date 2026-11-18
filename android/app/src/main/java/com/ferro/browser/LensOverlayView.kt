package com.ferro.browser

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.Path
import android.graphics.PorterDuff
import android.graphics.PorterDuffXfermode
import android.graphics.RectF
import android.util.Base64
import android.util.TypedValue
import android.view.Gravity
import android.view.MotionEvent
import android.view.View
import android.webkit.WebView
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.TextView
import android.graphics.drawable.GradientDrawable
import java.io.ByteArrayOutputStream
import kotlin.math.abs
import kotlin.math.max
import kotlin.math.min

/**
 * Chrome-exact Interactive Google Lens Region Selector Overlay for Android.
 * Draws a translucent dim scrim, spotlight cutout hole, 4 rounded white Chrome corner brackets,
 * and captures the selected Bitmap directly from the active WebView.
 */
class LensOverlayView(
    context: Context,
    private val targetWebView: WebView,
    private val onRegionCaptured: (base64Png: String) -> Unit,
    private val onCancel: () -> Unit
) : FrameLayout(context) {

    private val dp = resources.displayMetrics.density
    private val scrimPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        color = Color.argb(122, 10, 13, 22)
        style = Paint.Style.FILL
    }
    private val clearPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        xfermode = PorterDuffXfermode(PorterDuff.Mode.CLEAR)
    }
    private val bracketPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        color = Color.WHITE
        style = Paint.Style.STROKE
        strokeWidth = 3.5f * dp
        strokeCap = Paint.Cap.ROUND
        strokeJoin = Paint.Join.ROUND
    }
    private val borderGlowPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        color = Color.argb(95, 66, 133, 244)
        style = Paint.Style.STROKE
        strokeWidth = 1.5f * dp
    }

    private var startX = 0f
    private var startY = 0f
    private val selRect = RectF()
    private var hasSelection = false

    init {
        setLayerType(View.LAYER_TYPE_HARDWARE, null)
        setWillNotDraw(false)
        isClickable = true
        isFocusable = true

        // Top floating Chrome Lens instruction pill
        val pill = LinearLayout(context).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            setPadding((16 * dp).toInt(), (8 * dp).toInt(), (10 * dp).toInt(), (8 * dp).toInt())
            background = GradientDrawable().apply {
                setColor(Color.argb(240, 26, 27, 33))
                cornerRadius = 999f
                setStroke((1 * dp).toInt(), Color.argb(48, 255, 255, 255))
            }
            elevation = 10 * dp
        }

        val dot = View(context).apply {
            layoutParams = LinearLayout.LayoutParams((10 * dp).toInt(), (10 * dp).toInt()).apply {
                rightMargin = (10 * dp).toInt()
            }
            background = GradientDrawable().apply {
                shape = GradientDrawable.OVAL
                setColor(Color.parseColor("#4285F4"))
            }
        }
        pill.addView(dot)

        val label = TextView(context).apply {
            text = "Выделите область для Google Объектива"
            setTextColor(Color.parseColor("#F4F4F7"))
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 13f)
        }
        pill.addView(label)

        val closeBtn = TextView(context).apply {
            text = "✕"
            setTextColor(Color.parseColor("#E4E4E9"))
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 14f)
            gravity = Gravity.CENTER
            val sz = (28 * dp).toInt()
            layoutParams = LinearLayout.LayoutParams(sz, sz).apply {
                leftMargin = (10 * dp).toInt()
            }
            background = GradientDrawable().apply {
                shape = GradientDrawable.OVAL
                setColor(Color.argb(30, 255, 255, 255))
            }
            setOnClickListener { onCancel() }
        }
        pill.addView(closeBtn)

        val lp = LayoutParams(LayoutParams.WRAP_CONTENT, LayoutParams.WRAP_CONTENT).apply {
            gravity = Gravity.TOP or Gravity.CENTER_HORIZONTAL
            topMargin = (18 * dp).toInt()
        }
        addView(pill, lp)
    }

    override fun onDraw(canvas: Canvas) {
        super.onDraw(canvas)
        canvas.drawRect(0f, 0f, width.toFloat(), height.toFloat(), scrimPaint)

        if (hasSelection && selRect.width() > 6f && selRect.height() > 6f) {
            val r = 12f * dp
            canvas.drawRoundRect(selRect, r, r, clearPaint)
            canvas.drawRoundRect(selRect, r, r, borderGlowPaint)
            drawCornerBrackets(canvas, selRect, 22f * dp, 11f * dp)
        }
    }

    private fun drawCornerBrackets(canvas: Canvas, rect: RectF, arm: Float, rad: Float) {
        val l = rect.left
        val t = rect.top
        val r = rect.right
        val b = rect.bottom
        val cArm = min(arm, min(rect.width(), rect.height()) / 2f)
        val cRad = min(rad, cArm / 1.5f)

        val path = Path()
        // Top-Left
        path.moveTo(l, t + cArm)
        path.lineTo(l, t + cRad)
        path.quadTo(l, t, l + cRad, t)
        path.lineTo(l + cArm, t)

        // Top-Right
        path.moveTo(r - cArm, t)
        path.lineTo(r - cRad, t)
        path.quadTo(r, t, r, t + cRad)
        path.lineTo(r, t + cArm)

        // Bottom-Right
        path.moveTo(r, b - cArm)
        path.lineTo(r, b - cRad)
        path.quadTo(r, b, r - cRad, b)
        path.lineTo(r - cArm, b)

        // Bottom-Left
        path.moveTo(l + cArm, b)
        path.lineTo(l + cRad, b)
        path.quadTo(l, b, l, b - cRad)
        path.lineTo(l, b - cArm)

        canvas.drawPath(path, bracketPaint)
    }

    override fun onTouchEvent(event: MotionEvent): Boolean {
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                startX = event.x
                startY = event.y
                selRect.set(startX, startY, startX, startY)
                hasSelection = true
                invalidate()
                return true
            }
            MotionEvent.ACTION_MOVE -> {
                selRect.set(
                    min(startX, event.x).coerceIn(0f, width.toFloat()),
                    min(startY, event.y).coerceIn(0f, height.toFloat()),
                    max(startX, event.x).coerceIn(0f, width.toFloat()),
                    max(startY, event.y).coerceIn(0f, height.toFloat())
                )
                invalidate()
                return true
            }
            MotionEvent.ACTION_UP -> {
                val w = abs(selRect.width()).toInt()
                val h = abs(selRect.height()).toInt()
                if (w < (20 * dp).toInt() || h < (20 * dp).toInt()) {
                    hasSelection = false
                    invalidate()
                    return true
                }
                captureWebViewCrop()
                return true
            }
        }
        return super.onTouchEvent(event)
    }

    private fun captureWebViewCrop() {
        visibility = View.GONE
        post {
            try {
                val cropX = selRect.left.toInt().coerceIn(0, max(1, targetWebView.width - 16))
                val cropY = selRect.top.toInt().coerceIn(0, max(1, targetWebView.height - 16))
                val cropW = selRect.width().toInt().coerceIn(16, max(16, targetWebView.width - cropX))
                val cropH = selRect.height().toInt().coerceIn(16, max(16, targetWebView.height - cropY))

                val bmp = Bitmap.createBitmap(cropW, cropH, Bitmap.Config.ARGB_8888)
                val canvas = Canvas(bmp)
                canvas.translate((-cropX - targetWebView.scrollX).toFloat(), (-cropY - targetWebView.scrollY).toFloat())
                targetWebView.draw(canvas)

                // Downscale if huge for instant mobile upload
                val maxSide = 1100
                val finalBmp = if (bmp.width > maxSide || bmp.height > maxSide) {
                    val ratio = min(maxSide.toFloat() / bmp.width, maxSide.toFloat() / bmp.height)
                    Bitmap.createScaledBitmap(bmp, (bmp.width * ratio).toInt().coerceAtLeast(1), (bmp.height * ratio).toInt().coerceAtLeast(1), true)
                } else {
                    bmp
                }

                val out = ByteArrayOutputStream()
                finalBmp.compress(Bitmap.CompressFormat.PNG, 100, out)
                val b64 = Base64.encodeToString(out.toByteArray(), Base64.NO_WRAP)
                onRegionCaptured(b64)
            } catch (_: Exception) {
                onCancel()
            }
        }
    }
}
