package ua.elamon.ui

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.PathBuilder
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp

/** Outline icons the core Material set lacks, on the same 24-unit grid with a 2-unit stroke. */
object ElIcons {
	private class Scope(val b: ImageVector.Builder) {
		fun stroke(block: PathBuilder.() -> Unit) {
			b.path(
				stroke = SolidColor(Color.Black),
				strokeLineWidth = 2f,
				strokeLineCap = StrokeCap.Round,
				strokeLineJoin = StrokeJoin.Round,
				pathBuilder = block,
			)
		}

		fun fill(block: PathBuilder.() -> Unit) {
			b.path(fill = SolidColor(Color.Black), pathBuilder = block)
		}
	}

	private fun icon(name: String, block: Scope.() -> Unit): ImageVector =
		ImageVector.Builder(name, 24.dp, 24.dp, 24f, 24f).also { Scope(it).block() }.build()

	private fun PathBuilder.rect(l: Float, t: Float, r: Float, b: Float, rad: Float = 0f) {
		moveTo(l + rad, t)
		lineTo(r - rad, t)
		if (rad > 0) quadTo(r, t, r, t + rad)
		lineTo(r, b - rad)
		if (rad > 0) quadTo(r, b, r - rad, b)
		lineTo(l + rad, b)
		if (rad > 0) quadTo(l, b, l, b - rad)
		lineTo(l, t + rad)
		if (rad > 0) quadTo(l, t, l + rad, t)
		close()
	}

	private fun PathBuilder.circle(cx: Float, cy: Float, r: Float) {
		moveTo(cx - r, cy)
		arcTo(r, r, 0f, isMoreThanHalf = true, isPositiveArc = true, x1 = cx + r, y1 = cy)
		arcTo(r, r, 0f, isMoreThanHalf = true, isPositiveArc = true, x1 = cx - r, y1 = cy)
		close()
	}

	private fun PathBuilder.line(x1: Float, y1: Float, x2: Float, y2: Float) {
		moveTo(x1, y1)
		lineTo(x2, y2)
	}

	val Monitor = icon("Monitor") {
		stroke {
			rect(3f, 4f, 21f, 16f, 2f)
			line(12f, 16f, 12f, 20f)
			line(8f, 20f, 16f, 20f)
		}
	}

	val Laptop = icon("Laptop") {
		stroke {
			rect(5f, 5f, 19f, 15f, 1.5f)
			line(2f, 19f, 22f, 19f)
		}
	}

	val Thermometer = icon("Thermometer") {
		stroke {
			moveTo(10f, 14.5f)
			lineTo(10f, 5f)
			arcTo(2f, 2f, 0f, isMoreThanHalf = false, isPositiveArc = true, x1 = 14f, y1 = 5f)
			lineTo(14f, 14.5f)
			arcTo(4f, 4f, 0f, isMoreThanHalf = true, isPositiveArc = true, x1 = 10f, y1 = 14.5f)
			close()
		}
		fill {
			circle(12f, 18f, 1.75f)
			rect(11.25f, 9f, 12.75f, 18f)
		}
	}

	val Chip = icon("Chip") {
		stroke {
			rect(6f, 6f, 18f, 18f, 2f)
			for (p in floatArrayOf(9f, 12f, 15f)) {
				line(p, 6f, p, 3f)
				line(p, 18f, p, 21f)
				line(6f, p, 3f, p)
				line(18f, p, 21f, p)
			}
		}
		fill { rect(9.5f, 9.5f, 14.5f, 14.5f, 1f) }
	}

	val Memory = icon("Memory") {
		stroke {
			rect(3f, 6.5f, 21f, 15.5f, 1.5f)
			for (x in floatArrayOf(6f, 9f, 12f, 15f, 18f)) line(x, 15.5f, x, 18.5f)
		}
		fill {
			rect(6f, 9.5f, 8.5f, 12.5f)
			rect(10.75f, 9.5f, 13.25f, 12.5f)
			rect(15.5f, 9.5f, 18f, 12.5f)
		}
	}

	val Gpu = icon("Gpu") {
		stroke {
			rect(2f, 6f, 22f, 17f, 2f)
			circle(15f, 11.5f, 3.25f)
			line(6f, 17f, 6f, 20f)
			line(10f, 17f, 10f, 20f)
		}
		fill {
			rect(5f, 9f, 9f, 14f, 0.5f)
			circle(15f, 11.5f, 1f)
		}
	}

	val Network = icon("Network") {
		stroke {
			line(8f, 20f, 8f, 4f)
			moveTo(4f, 8f)
			lineTo(8f, 4f)
			lineTo(12f, 8f)
			line(16f, 4f, 16f, 20f)
			moveTo(12f, 16f)
			lineTo(16f, 20f)
			lineTo(20f, 16f)
		}
	}

	val Storage = icon("Storage") {
		stroke {
			rect(3f, 4f, 21f, 10.5f, 2f)
			rect(3f, 13.5f, 21f, 20f, 2f)
		}
		fill {
			circle(17f, 7.25f, 1.25f)
			circle(17f, 16.75f, 1.25f)
		}
	}

	val Battery = icon("Battery") {
		stroke { rect(2.5f, 7f, 19f, 17f, 2f) }
		fill {
			rect(20.5f, 10f, 22f, 14f, 0.5f)
			rect(5f, 9.5f, 12f, 14.5f, 0.5f)
		}
	}

	val Fan = icon("Fan") {
		fill {
			circle(12f, 12f, 2f)
			// Three curved blades, one drawn and turned by 120°.
			for (k in 0 until 3) {
				val a = Math.toRadians(120.0 * k)
				fun x(px: Float, py: Float) = (12 + (px - 12) * Math.cos(a) - (py - 12) * Math.sin(a)).toFloat()
				fun y(px: Float, py: Float) = (12 + (px - 12) * Math.sin(a) + (py - 12) * Math.cos(a)).toFloat()
				moveTo(x(12f, 10f), y(12f, 10f))
				quadTo(x(9f, 6f), y(9f, 6f), x(11f, 2.5f), y(11f, 2.5f))
				quadTo(x(17f, 3f), y(17f, 3f), x(14f, 10.5f), y(14f, 10.5f))
				close()
			}
		}
	}

	val Bolt = icon("Bolt") {
		fill {
			moveTo(13f, 2f)
			lineTo(4f, 14f)
			lineTo(11f, 14f)
			lineTo(10f, 22f)
			lineTo(20f, 9.5f)
			lineTo(13f, 9.5f)
			close()
		}
	}

	val Processes = icon("Processes") {
		stroke {
			line(9f, 6f, 20f, 6f)
			line(9f, 12f, 20f, 12f)
			line(9f, 18f, 20f, 18f)
		}
		fill {
			circle(4.5f, 6f, 1.4f)
			circle(4.5f, 12f, 1.4f)
			circle(4.5f, 18f, 1.4f)
		}
	}

	val Overview = icon("Overview") {
		stroke {
			rect(3.5f, 3.5f, 10.5f, 12.5f, 1.5f)
			rect(13.5f, 3.5f, 20.5f, 8.5f, 1.5f)
			rect(3.5f, 15.5f, 10.5f, 20.5f, 1.5f)
			rect(13.5f, 11.5f, 20.5f, 20.5f, 1.5f)
		}
	}

	val Copy = icon("Copy") {
		stroke {
			rect(8f, 8f, 20f, 20f, 2f)
			moveTo(16f, 8f)
			lineTo(16f, 6f)
			quadTo(16f, 4f, 14f, 4f)
			lineTo(6f, 4f)
			quadTo(4f, 4f, 4f, 6f)
			lineTo(4f, 14f)
			quadTo(4f, 16f, 6f, 16f)
			lineTo(8f, 16f)
		}
	}

	val Help = icon("Help") {
		stroke {
			circle(12f, 12f, 9.5f)
			moveTo(9.25f, 9.5f)
			quadTo(9.5f, 6.75f, 12f, 6.75f)
			quadTo(14.75f, 6.75f, 14.75f, 9.25f)
			quadTo(14.75f, 11f, 12f, 12.25f)
			lineTo(12f, 13.75f)
		}
		fill { circle(12f, 17f, 1.25f) }
	}

	val Qr = icon("Qr") {
		stroke {
			rect(3.5f, 3.5f, 9.5f, 9.5f, 1f)
			rect(14.5f, 3.5f, 20.5f, 9.5f, 1f)
			rect(3.5f, 14.5f, 9.5f, 20.5f, 1f)
		}
		fill {
			rect(5.5f, 5.5f, 7.5f, 7.5f)
			rect(16.5f, 5.5f, 18.5f, 7.5f)
			rect(5.5f, 16.5f, 7.5f, 18.5f)
			rect(13.5f, 13.5f, 16.5f, 16.5f)
			rect(18f, 18f, 21f, 21f)
			rect(18f, 13.5f, 21f, 15.5f)
			rect(13.5f, 18f, 15.5f, 21f)
		}
	}
}
