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
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Dp

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
				.padding(horizontal = Space.xl, vertical = Space.lPlus),
			verticalArrangement = Arrangement.spacedBy(Space.mPlus),
		) {
			Row(verticalAlignment = Alignment.CenterVertically) {
				IconBadge(icon, accent)
				Spacer(Modifier.width(Space.m))
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
fun IconBadge(icon: ImageVector, accent: Color, size: Dp = Sizes.badge) {
	Box(
		Modifier.size(size).clip(CircleShape).background(accent.copy(alpha = Alpha.BADGE)),
		contentAlignment = Alignment.Center,
	) {
		Icon(icon, null, tint = accent, modifier = Modifier.size(size * Sizes.BADGE_ICON))
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
				.background(Brush.linearGradient(listOf(accent.copy(alpha = Alpha.TILE_FROM), accent.copy(alpha = Alpha.TILE_TO))))
				.then(if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier)
				.padding(Space.l),
			verticalArrangement = Arrangement.spacedBy(Space.s),
			content = content,
		)
	}
}

/** The small caption over a tile's big number. */
@Composable
fun TileLabel(text: String, icon: ImageVector, accent: Color) {
	Row(verticalAlignment = Alignment.CenterVertically) {
		Icon(icon, null, tint = accent, modifier = Modifier.size(Sizes.iconS))
		Spacer(Modifier.width(Space.xsPlus))
		Text(text, style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1)
	}
}

/** A big number with its unit set smaller beside it. */
@Composable
fun BigValue(value: String, unit: String? = null, color: Color = MaterialTheme.colorScheme.onSurface, large: Boolean = true) {
	Row(verticalAlignment = Alignment.Bottom) {
		RollingText(value, if (large) MaterialTheme.typography.headlineLarge else MaterialTheme.typography.headlineSmall, color = color)
		if (unit != null) {
			Spacer(Modifier.width(Space.unitGap))
			Text(
				unit,
				style = MaterialTheme.typography.titleSmall,
				color = MaterialTheme.colorScheme.onSurfaceVariant,
				modifier = Modifier.padding(bottom = if (large) Space.unitLift else Space.unitLiftSmall),
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

/**
 * "Label ........ value" on one line where both fit whole; where they don't (large text, a long
 * value), the value goes under the label, so neither is cut short.
 */
@Composable
fun StatRow(label: String, value: String, modifier: Modifier = Modifier, valueColor: Color = MaterialTheme.colorScheme.onSurface) {
	Layout(
		content = {
			Text(label, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
			Text(value, style = MaterialTheme.typography.bodyMedium, color = valueColor, fontWeight = FontWeight.Medium, textAlign = TextAlign.End)
		},
		modifier = modifier.fillMaxWidth(),
	) { measurables, constraints ->
		val (labelText, valueText) = measurables
		val gap = Space.m.roundToPx()
		val labelWidth = labelText.maxIntrinsicWidth(Constraints.Infinity)
		val valueWidth = valueText.maxIntrinsicWidth(Constraints.Infinity)
		val width = if (constraints.hasBoundedWidth) constraints.maxWidth else labelWidth + gap + valueWidth
		val loose = Constraints(maxWidth = width)
		if (labelWidth + gap + valueWidth <= width) {
			val l = labelText.measure(loose)
			val v = valueText.measure(loose)
			val height = maxOf(l.height, v.height)
			layout(width, height) {
				l.placeRelative(0, (height - l.height) / 2)
				v.placeRelative(width - v.width, (height - v.height) / 2)
			}
		} else {
			val l = labelText.measure(loose)
			val v = valueText.measure(loose)
			layout(width, l.height + v.height) {
				l.placeRelative(0, 0)
				v.placeRelative(width - v.width, l.height)
			}
		}
	}
}

/**
 * A small rounded tag, like "Charging" or "Sleeping": tinted by [color], its text in [color] too,
 * darkened (lightened on dark themes) where that alone wouldn't read on the tint.
 */
@Composable
fun Pill(text: String, color: Color = MaterialTheme.colorScheme.primary, modifier: Modifier = Modifier, dot: Boolean = false) {
	val scheme = MaterialTheme.colorScheme
	val dark = LocalDark.current
	val ink = remember(color, scheme, dark) { pillTextColor(color, pillSurfaces(scheme), dark) }
	Row(
		modifier.clip(PillShape).background(color.copy(alpha = Alpha.PILL)).padding(horizontal = Space.sPlus, vertical = Space.xs),
		verticalAlignment = Alignment.CenterVertically,
	) {
		if (dot) {
			Box(Modifier.size(Sizes.pillDot).clip(CircleShape).background(color))
			Spacer(Modifier.width(Space.xsPlus))
		}
		Text(text, style = MaterialTheme.typography.labelMedium, color = ink, maxLines = 1)
	}
}

/** A coloured dot and a name, for chart legends. */
@Composable
fun Legend(color: Color, text: String, modifier: Modifier = Modifier) {
	Row(modifier, verticalAlignment = Alignment.CenterVertically) {
		Box(Modifier.size(Sizes.legendDot).clip(CircleShape).background(color))
		Spacer(Modifier.width(Space.xsPlus))
		Text(text, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1)
	}
}

/** A message in the middle of an empty or failed screen. */
@Composable
fun Notice(icon: ImageVector, title: String, body: String?, modifier: Modifier = Modifier, action: (@Composable () -> Unit)? = null) {
	Column(
		modifier.fillMaxWidth().padding(Space.xxxl),
		horizontalAlignment = Alignment.CenterHorizontally,
		verticalArrangement = Arrangement.spacedBy(Space.m),
	) {
		IconBadge(icon, MaterialTheme.colorScheme.primary, Sizes.badgeNotice)
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
		modifier = modifier.padding(start = Space.xl, top = Space.m, bottom = Space.xxs),
	)
}

/** Rows on one rounded card, divided by hairlines. */
@Composable
fun ListGroup(modifier: Modifier = Modifier, content: @Composable ColumnScope.() -> Unit) {
	Surface(modifier = modifier.fillMaxWidth(), shape = CardShape, color = MaterialTheme.colorScheme.surfaceContainerLow) {
		Column(Modifier.padding(vertical = Space.xs), content = content)
	}
}

@Composable
fun GroupDivider() {
	HorizontalDivider(Modifier.padding(horizontal = Space.xl), color = MaterialTheme.colorScheme.outlineVariant.copy(alpha = Alpha.DIVIDER))
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
	RowLayout(
		title, summary, icon, accent, enabled,
		modifier = if (onClick != null && enabled) Modifier.clickable(onClick = onClick) else Modifier,
		trailing = trailing,
		chevron = onClick != null,
	)
}

/** A setting turned on and off: the whole row is the switch, one control to a screen reader. */
@Composable
fun SwitchRow(title: String, summary: String? = null, checked: Boolean, enabled: Boolean = true, onChange: (Boolean) -> Unit) {
	RowLayout(
		title, summary, null, MaterialTheme.colorScheme.primary, enabled,
		modifier = Modifier.toggleable(value = checked, enabled = enabled, role = Role.Switch, onValueChange = onChange),
		trailing = { Switch(checked = checked, onCheckedChange = null, enabled = enabled) },
		chevron = false,
	)
}

@Composable
private fun RowLayout(
	title: String,
	summary: String?,
	icon: ImageVector?,
	accent: Color,
	enabled: Boolean,
	trailing: (@Composable () -> Unit)?,
	chevron: Boolean,
	/** What the row does when touched: a click, a toggle, or nothing. */
	modifier: Modifier = Modifier,
) {
	val alpha = if (enabled) 1f else Alpha.DISABLED
	Row(
		modifier
			.fillMaxWidth()
			.padding(horizontal = Space.xl, vertical = Space.mPlus),
		verticalAlignment = Alignment.CenterVertically,
	) {
		if (icon != null) {
			IconBadge(icon, accent.copy(alpha = alpha), Sizes.badgeRow)
			Spacer(Modifier.width(Space.mPlus))
		}
		Column(Modifier.weight(1f)) {
			Text(title, style = MaterialTheme.typography.bodyLarge, color = MaterialTheme.colorScheme.onSurface.copy(alpha = alpha))
			if (summary != null) {
				Text(summary, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = alpha))
			}
		}
		if (trailing != null) {
			Spacer(Modifier.width(Space.m))
			trailing()
		} else if (chevron) {
			Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, null, tint = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = alpha))
		}
	}
}

/** A name's own hue, from 0 to 359, the same every time; never negative, whatever the hash. */
fun avatarHue(name: String): Float = Math.floorMod(name.lowercase().hashCode(), HUES).toFloat()

private const val HUES = 360

/** A letter avatar's disc and letter, dark and light: saturation, then lightness. */
private const val AVATAR_BG_SATURATION = 0.55f
private const val AVATAR_BG_DARK = 0.32f
private const val AVATAR_BG_LIGHT = 0.86f
private const val AVATAR_FG_SATURATION = 0.7f
private const val AVATAR_FG_DARK = 0.84f
private const val AVATAR_FG_LIGHT = 0.3f

/** A coloured disc with a name's first letter: the stand-in for icons we can't have. */
@Composable
fun LetterAvatar(name: String, size: Dp = Sizes.avatar) {
	val hue = avatarHue(name)
	val dark = LocalDark.current
	val bg = Color.hsl(hue, AVATAR_BG_SATURATION, if (dark) AVATAR_BG_DARK else AVATAR_BG_LIGHT)
	val fg = Color.hsl(hue, AVATAR_FG_SATURATION, if (dark) AVATAR_FG_DARK else AVATAR_FG_LIGHT)
	Box(Modifier.size(size).clip(CircleShape).background(bg), contentAlignment = Alignment.Center) {
		Text(
			name.trim().firstOrNull()?.uppercase() ?: "?",
			style = if (size >= Sizes.avatar) MaterialTheme.typography.titleMedium else MaterialTheme.typography.labelLarge,
			color = fg,
		)
	}
}
