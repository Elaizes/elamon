package ua.elamon.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import kotlin.math.abs

val CardShape = RoundedCornerShape(26.dp)

/** A titled card: the building block of every detail page. */
@Composable
fun Section(
	title: String,
	icon: ImageVector,
	modifier: Modifier = Modifier,
	accent: Color = MaterialTheme.colorScheme.primary,
	trailing: (@Composable () -> Unit)? = null,
	onClick: (() -> Unit)? = null,
	content: @Composable ColumnScope.() -> Unit,
) {
	Surface(modifier = modifier.fillMaxWidth(), shape = CardShape, color = MaterialTheme.colorScheme.surfaceContainerLow) {
		Column(
			Modifier
				.then(if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier)
				.padding(horizontal = 20.dp, vertical = 18.dp),
			verticalArrangement = Arrangement.spacedBy(14.dp),
		) {
			Row(verticalAlignment = Alignment.CenterVertically) {
				IconBadge(icon, accent)
				Spacer(Modifier.width(12.dp))
				Text(
					title,
					style = MaterialTheme.typography.titleMedium,
					modifier = Modifier.weight(1f),
					maxLines = 1,
					overflow = TextOverflow.Ellipsis,
				)
				trailing?.invoke()
			}
			content()
		}
	}
}

/** An icon on a tinted disc. */
@Composable
fun IconBadge(icon: ImageVector, accent: Color, size: Dp = 34.dp) {
	Box(
		Modifier.size(size).clip(CircleShape).background(accent.copy(alpha = 0.16f)),
		contentAlignment = Alignment.Center,
	) {
		Icon(icon, null, tint = accent, modifier = Modifier.size(size * 0.56f))
	}
}

/** A dashboard tile: tinted by its accent, the whole of it a button. */
@Composable
fun Tile(
	modifier: Modifier = Modifier,
	accent: Color = MaterialTheme.colorScheme.primary,
	onClick: (() -> Unit)? = null,
	content: @Composable ColumnScope.() -> Unit,
) {
	val base = MaterialTheme.colorScheme.surfaceContainerLow
	Surface(modifier = modifier, shape = CardShape, color = base) {
		Column(
			Modifier
				.background(Brush.linearGradient(listOf(accent.copy(alpha = 0.13f), accent.copy(alpha = 0.02f))))
				.then(if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier)
				.padding(16.dp),
			verticalArrangement = Arrangement.spacedBy(8.dp),
			content = content,
		)
	}
}

/** The small caption over a tile's big number. */
@Composable
fun TileLabel(text: String, icon: ImageVector, accent: Color) {
	Row(verticalAlignment = Alignment.CenterVertically) {
		Icon(icon, null, tint = accent, modifier = Modifier.size(16.dp))
		Spacer(Modifier.width(6.dp))
		Text(text, style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1)
	}
}

/** A big number with its unit set smaller beside it. */
@Composable
fun BigValue(value: String, unit: String? = null, color: Color = MaterialTheme.colorScheme.onSurface, large: Boolean = true) {
	Row(verticalAlignment = Alignment.Bottom) {
		RollingText(value, if (large) MaterialTheme.typography.headlineLarge else MaterialTheme.typography.headlineSmall, color = color)
		if (unit != null) {
			Spacer(Modifier.width(3.dp))
			Text(
				unit,
				style = MaterialTheme.typography.titleSmall,
				color = MaterialTheme.colorScheme.onSurfaceVariant,
				modifier = Modifier.padding(bottom = if (large) 5.dp else 3.dp),
			)
		}
	}
}

/** A label above a number, the way dashboards show their main figures. */
@Composable
fun BigStat(label: String, value: String, modifier: Modifier = Modifier, color: Color = MaterialTheme.colorScheme.onSurface) {
	Column(modifier) {
		Text(label, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1)
		RollingText(value, MaterialTheme.typography.titleLarge, color = color)
	}
}

/** "Label ........ value" */
@Composable
fun StatRow(label: String, value: String, modifier: Modifier = Modifier, valueColor: Color = MaterialTheme.colorScheme.onSurface) {
	Row(modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
		Text(
			label,
			style = MaterialTheme.typography.bodyMedium,
			color = MaterialTheme.colorScheme.onSurfaceVariant,
			modifier = Modifier.weight(1f),
			maxLines = 1,
			overflow = TextOverflow.Ellipsis,
		)
		Spacer(Modifier.width(12.dp))
		Text(value, style = MaterialTheme.typography.bodyMedium, color = valueColor, maxLines = 1, fontWeight = FontWeight.Medium)
	}
}

/** A small rounded tag, like "Charging" or "Sleeping". */
@Composable
fun Pill(text: String, color: Color = MaterialTheme.colorScheme.primary, modifier: Modifier = Modifier, dot: Boolean = false) {
	Row(
		modifier.clip(RoundedCornerShape(50)).background(color.copy(alpha = 0.15f)).padding(horizontal = 10.dp, vertical = 4.dp),
		verticalAlignment = Alignment.CenterVertically,
	) {
		if (dot) {
			Box(Modifier.size(6.dp).clip(CircleShape).background(color))
			Spacer(Modifier.width(6.dp))
		}
		Text(text, style = MaterialTheme.typography.labelMedium, color = color, maxLines = 1)
	}
}

/** A coloured dot and a name, for chart legends. */
@Composable
fun Legend(color: Color, text: String, modifier: Modifier = Modifier) {
	Row(modifier, verticalAlignment = Alignment.CenterVertically) {
		Box(Modifier.size(8.dp).clip(CircleShape).background(color))
		Spacer(Modifier.width(6.dp))
		Text(text, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1)
	}
}

/** A message in the middle of an empty or failed screen. */
@Composable
fun Notice(icon: ImageVector, title: String, body: String?, modifier: Modifier = Modifier, action: (@Composable () -> Unit)? = null) {
	Column(
		modifier.fillMaxWidth().padding(32.dp),
		horizontalAlignment = Alignment.CenterHorizontally,
		verticalArrangement = Arrangement.spacedBy(12.dp),
	) {
		IconBadge(icon, MaterialTheme.colorScheme.primary, 64.dp)
		Text(title, style = MaterialTheme.typography.titleLarge, textAlign = TextAlign.Center)
		if (body != null) {
			Text(body, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant, textAlign = TextAlign.Center)
		}
		action?.invoke()
	}
}

/** A caption over a group, One UI style: small, coloured, indented. */
@Composable
fun GroupLabel(text: String, modifier: Modifier = Modifier) {
	Text(
		text,
		style = MaterialTheme.typography.labelLarge,
		color = MaterialTheme.colorScheme.primary,
		modifier = modifier.padding(start = 20.dp, top = 12.dp, bottom = 2.dp),
	)
}

/** Rows on one rounded card, divided by hairlines. */
@Composable
fun ListGroup(modifier: Modifier = Modifier, content: @Composable ColumnScope.() -> Unit) {
	Surface(modifier = modifier.fillMaxWidth(), shape = CardShape, color = MaterialTheme.colorScheme.surfaceContainerLow) {
		Column(Modifier.padding(vertical = 4.dp), content = content)
	}
}

@Composable
fun GroupDivider() {
	HorizontalDivider(Modifier.padding(horizontal = 20.dp), color = MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.4f))
}

/** A row in a list group: title, optional summary, and a trailing control or a chevron. */
@Composable
fun ListRow(
	title: String,
	summary: String? = null,
	icon: ImageVector? = null,
	accent: Color = MaterialTheme.colorScheme.primary,
	enabled: Boolean = true,
	onClick: (() -> Unit)? = null,
	trailing: (@Composable () -> Unit)? = null,
) {
	val alpha = if (enabled) 1f else 0.45f
	Row(
		Modifier
			.fillMaxWidth()
			.then(if (onClick != null && enabled) Modifier.clickable(onClick = onClick) else Modifier)
			.padding(horizontal = 20.dp, vertical = 14.dp),
		verticalAlignment = Alignment.CenterVertically,
	) {
		if (icon != null) {
			IconBadge(icon, accent.copy(alpha = alpha), 36.dp)
			Spacer(Modifier.width(14.dp))
		}
		Column(Modifier.weight(1f)) {
			Text(title, style = MaterialTheme.typography.bodyLarge, color = MaterialTheme.colorScheme.onSurface.copy(alpha = alpha))
			if (summary != null) {
				Text(summary, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = alpha))
			}
		}
		if (trailing != null) {
			Spacer(Modifier.width(12.dp))
			trailing()
		} else if (onClick != null) {
			Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, null, tint = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = alpha))
		}
	}
}

@Composable
fun SwitchRow(title: String, summary: String? = null, checked: Boolean, enabled: Boolean = true, onChange: (Boolean) -> Unit) {
	ListRow(title, summary, enabled = enabled, onClick = { onChange(!checked) }, trailing = {
		Switch(checked = checked, onCheckedChange = onChange, enabled = enabled)
	})
}

/** A status dot: green when up, red when down, grey when unknown. */
@Composable
fun StatusDot(up: Boolean?, modifier: Modifier = Modifier) {
	val c = when (up) {
		true -> Metric.colors.good
		false -> Metric.colors.hot
		null -> MaterialTheme.colorScheme.outline
	}
	Box(modifier.size(9.dp).clip(CircleShape).background(c))
}

/** A coloured disc with a name's first letter: the stand-in for icons we can't have. */
@Composable
fun LetterAvatar(name: String, size: Dp = 40.dp) {
	val hue = (abs(name.lowercase().hashCode()) % 360).toFloat()
	val dark = LocalDark.current
	val bg = Color.hsl(hue, 0.55f, if (dark) 0.32f else 0.86f)
	val fg = Color.hsl(hue, 0.7f, if (dark) 0.84f else 0.3f)
	Box(Modifier.size(size).clip(CircleShape).background(bg), contentAlignment = Alignment.Center) {
		Text(
			name.trim().firstOrNull()?.uppercase() ?: "?",
			style = if (size >= 40.dp) MaterialTheme.typography.titleMedium else MaterialTheme.typography.labelLarge,
			color = fg,
		)
	}
}
