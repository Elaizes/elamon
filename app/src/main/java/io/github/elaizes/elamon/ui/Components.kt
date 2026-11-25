package io.github.elaizes.elamon.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.layout.layout
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Dp

/** How many cards this is in: none on the screen, one in a group, where what would be a card is a tile of it. */
val LocalDepth = compositionLocalOf { 0 }

/** The colour of a card here, or of what sits inside one: a step further from the page with each. */
@Composable
fun cardColor(): Color = when (LocalDepth.current) {
	0 -> MaterialTheme.colorScheme.surfaceContainerLow
	1 -> MaterialTheme.colorScheme.surfaceContainerHigh
	else -> MaterialTheme.colorScheme.surfaceContainerHighest
}

/** What a card here keeps between its edge and what it holds. */
@Composable
fun cardPadding(): Dp = if (LocalDepth.current == 0) Space.card else Space.m

/** A card: what every screen is built of. In another, one of its tiles, its corner inside that one's. */
@Composable
fun CardSurface(modifier: Modifier = Modifier, color: Color = cardColor(), content: @Composable () -> Unit) {
	val depth = LocalDepth.current
	Surface(modifier = modifier.fillMaxWidth(), shape = if (depth == 0) CardShape else InnerShape, color = color) {
		CompositionLocalProvider(LocalDepth provides depth + 1, content = content)
	}
}

/** A titled card: the building block of every detail page. */
@Composable
fun Section(
	title: String,
	icon: ImageVector,
	modifier: Modifier = Modifier,
	accent: Color = MaterialTheme.colorScheme.primary,
	trailing: (@Composable () -> Unit)? = null,
	onClick: (() -> Unit)? = null,
	/** What touching the title row does (folding the card), apart from the rest of it. */
	onTitleClick: (() -> Unit)? = null,
	content: @Composable ColumnScope.() -> Unit,
) {
	val padding = cardPadding()
	// In another card, and where a disc would leave the title a few letters, its icon alone.
	val inner = LocalDepth.current > 0 || narrowCell(TextSizes.rowLeast)
	CardSurface(modifier) {
		Column(
			Modifier
				.then(if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier)
				.padding(padding),
			verticalArrangement = Arrangement.spacedBy(Space.m),
		) {
			Row(
				// As tall with a figure at its end as without: the titles of a row stand on one line.
				Modifier.heightIn(min = Sizes.icon).then(if (onTitleClick != null) Modifier.clip(InnerShape).clickable(onClick = onTitleClick) else Modifier),
				verticalAlignment = Alignment.CenterVertically,
			) {
				if (inner) Icon(icon, null, tint = accent, modifier = Modifier.size(Sizes.iconM)) else IconBadge(icon, accent)
				Spacer(Modifier.width(if (inner) Space.s else Space.m))
				Text(
					title,
					style = if (inner) MaterialTheme.typography.titleSmall else MaterialTheme.typography.titleMedium,
					modifier = Modifier.weight(1f),
					maxLines = 1,
					overflow = TextOverflow.Ellipsis,
				)
				if (trailing != null) {
					Spacer(Modifier.width(Space.s))
					trailing()
				}
			}
			content()
		}
	}
}

@Composable
fun IconBadge(icon: ImageVector, accent: Color, size: Dp = Sizes.badge) {
	Box(
		Modifier.size(size).clip(CircleShape).background(accent.copy(alpha = accent.alpha * Alpha.BADGE)),
		contentAlignment = Alignment.Center,
	) {
		Icon(icon, null, tint = accent, modifier = Modifier.size(size * Sizes.BADGE_ICON))
	}
}

/** The mark at the end of a row that opens something, its point on the line the row ends on. */
@Composable
fun Chevron(tint: Color = MaterialTheme.colorScheme.onSurfaceVariant) {
	Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, null, tint = tint, modifier = Modifier.optical(Sizes.chevronEmpty))
}

/**
 * A dashboard tile: tinted by its accent, the whole of it a button. Its [caption] is its last line, at
 * its bottom whatever the tile beside it holds: the captions of a row stand on one line.
 */
@Composable
fun Tile(
	modifier: Modifier = Modifier,
	accent: Color = MaterialTheme.colorScheme.primary,
	onClick: (() -> Unit)? = null,
	caption: String? = null,
	content: @Composable ColumnScope.() -> Unit,
) {
	val padding = cardPadding()
	CardSurface(modifier) {
		Footed(
			Modifier
				.fillMaxWidth()
				.background(Brush.verticalGradient(listOf(accent.copy(alpha = Alpha.TILE_FROM), accent.copy(alpha = Alpha.TILE_TO))))
				.then(if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier)
				.padding(padding),
			gap = Space.s,
			main = { Column(verticalArrangement = Arrangement.spacedBy(Space.s), content = content) },
			last = { if (caption != null) TileCaption(caption) },
		)
	}
}

@Composable
private fun TileCaption(text: String) {
	// One line, so a figure in it that grows doesn't make the tile taller; large text has two.
	val large = LocalDensity.current.fontScale > LARGE_TEXT
	Text(
		text,
		style = MaterialTheme.typography.bodySmall,
		color = MaterialTheme.colorScheme.onSurfaceVariant,
		maxLines = if (large) 2 else 1,
		overflow = TextOverflow.Ellipsis,
		modifier = if (large) Modifier.wholeWords() else Modifier.shrinkToFit(Shrink.LABEL),
	)
}

/** From this scale of the phone's text on, captions have a second line. */
private const val LARGE_TEXT = 1.3f

/** The small caption over a tile's big number. */
@Composable
fun TileLabel(text: String, icon: ImageVector, accent: Color) {
	Row(verticalAlignment = Alignment.CenterVertically) {
		Icon(icon, null, tint = accent, modifier = Modifier.size(Sizes.iconS))
		Spacer(Modifier.width(Space.s))
		Text(text, style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.shrinkToFit(Shrink.LABEL))
	}
}

/** A big number with its unit set smaller beside it, on its baseline; smaller both where they don't fit. */
@Composable
fun BigValue(value: String, unit: String? = null, modifier: Modifier = Modifier, color: Color = MaterialTheme.colorScheme.onSurface, large: Boolean = true) {
	Row(modifier.shrinkToFit()) {
		RollingText(value, if (large) MaterialTheme.typography.headlineLarge else MaterialTheme.typography.headlineSmall, Modifier.alignByBaseline(), color = color)
		if (unit != null) {
			Spacer(Modifier.width(Space.xs))
			Text(unit, style = MaterialTheme.typography.titleSmall, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1, modifier = Modifier.alignByBaseline())
		}
	}
}

/** The line of a tile's figure, whatever stands on it (a number, a word, a dash): on one baseline, as tall. */
@Composable
fun Modifier.figureLine(): Modifier = onLine(TextSizes.figureLine, TextSizes.figureBaseline)

/** A label above a number, the way dashboards show their main figures. */
@Composable
fun BigStat(label: String, value: String, modifier: Modifier = Modifier, color: Color = MaterialTheme.colorScheme.onSurface) {
	Column(modifier) {
		Text(label, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1, overflow = TextOverflow.Ellipsis)
		RollingText(value, MaterialTheme.typography.titleLarge, Modifier.shrinkToFit(), color = color)
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
 * A small rounded tag, like "Charging" or "Asleep": tinted by [color], its text in [color] too,
 * darkened (lightened on dark themes) where that alone wouldn't read on the tint.
 */
@Composable
fun Pill(text: String, color: Color = MaterialTheme.colorScheme.primary, modifier: Modifier = Modifier, dot: Boolean = false) {
	val scheme = MaterialTheme.colorScheme
	val dark = LocalDark.current
	val ink = remember(color, scheme, dark) { pillTextColor(color, pillSurfaces(scheme), dark) }
	Row(
		modifier.clip(PillShape).background(color.copy(alpha = Alpha.PILL)).padding(horizontal = Space.m, vertical = Space.xs),
		verticalAlignment = Alignment.CenterVertically,
	) {
		if (dot) {
			Box(Modifier.size(Sizes.pillDot).clip(CircleShape).background(color))
			Spacer(Modifier.width(Space.s))
		}
		Text(text, style = MaterialTheme.typography.labelMedium, color = ink, maxLines = 1, overflow = TextOverflow.Ellipsis)
	}
}

@Composable
fun Legend(color: Color, text: String, modifier: Modifier = Modifier) {
	Row(modifier, verticalAlignment = Alignment.CenterVertically) {
		Box(Modifier.size(Sizes.legendDot).clip(CircleShape).background(color))
		Spacer(Modifier.width(Space.s))
		Text(text, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1, overflow = TextOverflow.Ellipsis)
	}
}

/**
 * A note in its colour over what a screen shows: what is the matter, what may be done about it at its
 * end ([trailing], a round button) or under it ([more]). A computer that can't be reached, control
 * that is off and a version too old are all told this way, at the top of the screen.
 */
@Composable
fun Banner(
	icon: ImageVector,
	color: Color,
	text: String,
	modifier: Modifier = Modifier,
	title: String? = null,
	trailing: (@Composable () -> Unit)? = null,
	more: (@Composable ColumnScope.() -> Unit)? = null,
) {
	CardSurface(modifier) {
		Column(
			Modifier.background(color.copy(alpha = Alpha.BANNER)).padding(horizontal = Space.card, vertical = if (trailing == null) Space.card else Space.xs),
			verticalArrangement = Arrangement.spacedBy(Space.m),
		) {
			Row(verticalAlignment = if (trailing == null) Alignment.Top else Alignment.CenterVertically) {
				FirstLine(MaterialTheme.typography.bodyMedium) { Icon(icon, null, tint = color, modifier = Modifier.size(Sizes.iconM)) }
				Spacer(Modifier.width(Space.m))
				Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(Space.xs)) {
					if (title != null) Text(title, style = MaterialTheme.typography.titleSmall)
					Text(text, style = MaterialTheme.typography.bodyMedium)
				}
				if (trailing != null) Box(Modifier.optical(Sizes.buttonEmpty)) { trailing() }
			}
			more?.invoke(this)
		}
	}
}

/** Centred on the first line of the text beside it, set in [style], however many lines that takes. */
@Composable
fun FirstLine(style: TextStyle, content: @Composable () -> Unit) {
	val line = with(LocalDensity.current) { style.lineHeight.toDp() }
	Box(Modifier.heightIn(min = line), contentAlignment = Alignment.Center) { content() }
}

/** A message in the middle of an empty or failed screen. */
@Composable
fun Notice(icon: ImageVector, title: String, body: String?, modifier: Modifier = Modifier, action: (@Composable () -> Unit)? = null) {
	Column(
		modifier.fillMaxWidth().padding(Space.xxl),
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
		modifier = modifier.padding(start = Space.card, end = Space.card, top = Space.m, bottom = Space.xs),
	)
}

/** Rows on one rounded card, divided by hairlines. */
@Composable
fun ListGroup(modifier: Modifier = Modifier, content: @Composable ColumnScope.() -> Unit) {
	CardSurface(modifier) { Column(Modifier.padding(vertical = Space.card - RowPadding), content = content) }
}

/** Over and under a row's text: with the group's own, a card's padding. */
private val RowPadding = Space.m

/**
 * Chips side by side, scrolling where they don't all fit: from edge to edge of the card they are in
 * ([padding], what it keeps on its sides), so one half seen is cut by the card and not short of it.
 */
@Composable
fun ChipRow(padding: Dp = Space.card, content: @Composable RowScope.() -> Unit) {
	Row(
		Modifier
			.layout { measurable, constraints ->
				if (!constraints.hasBoundedWidth) {
					val p = measurable.measure(constraints)
					return@layout layout(p.width, p.height) { p.place(0, 0) }
				}
				val side = padding.roundToPx()
				val wide = measurable.measure(constraints.copy(minWidth = 0, maxWidth = constraints.maxWidth + side * 2))
				layout(constraints.maxWidth, wide.height) { wide.place(-side, 0) }
			}
			.fillMaxWidth()
			.horizontalScroll(rememberScrollState())
			.padding(horizontal = padding),
		horizontalArrangement = Arrangement.spacedBy(Space.s),
		content = content,
	)
}

@Composable
fun GroupDivider() {
	HorizontalDivider(Modifier.padding(horizontal = Space.card), color = MaterialTheme.colorScheme.outlineVariant.copy(alpha = Alpha.DIVIDER))
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
	/** A spinner in the icon's place, while what the row started runs. */
	busy: Boolean = false,
	/**
	 * Under the title and summary: how what the row ran ended. Beside the title, in a row half a
	 * narrow screen wide, it left the title a few letters a line.
	 */
	below: (@Composable () -> Unit)? = null,
) {
	RowLayout(
		title, summary, icon, accent, enabled,
		modifier = if (onClick != null && enabled) Modifier.clickable(onClick = onClick) else Modifier,
		trailing = trailing,
		chevron = onClick != null,
		busy = busy,
		below = below,
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
		busy = false,
		below = null,
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
	busy: Boolean,
	below: (@Composable () -> Unit)?,
	/** What the row does when touched: a click, a toggle, or nothing. */
	modifier: Modifier = Modifier,
) {
	val alpha = if (enabled) 1f else Alpha.DISABLED
	Row(
		modifier
			.fillMaxWidth()
			.heightIn(min = Sizes.touchTarget)
			.padding(horizontal = Space.card, vertical = RowPadding),
		verticalAlignment = Alignment.CenterVertically,
	) {
		if (icon != null) {
			BusyBadge(icon, accent.copy(alpha = alpha), busy)
			Spacer(Modifier.width(Space.m))
		}
		Column(Modifier.weight(1f)) {
			Text(title, style = MaterialTheme.typography.bodyLarge, color = MaterialTheme.colorScheme.onSurface.copy(alpha = alpha), modifier = Modifier.wholeWords())
			if (summary != null) {
				Text(summary, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = alpha))
			}
			if (below != null) {
				Spacer(Modifier.height(Space.xs))
				below()
			}
		}
		if (trailing != null) {
			Spacer(Modifier.width(Space.m))
			trailing()
		} else if (chevron) {
			Spacer(Modifier.width(Space.xs))
			Chevron(MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = alpha))
		}
	}
}

/** A row's icon in its disc, or while what the row started runs, a spinner as large. */
@Composable
fun BusyBadge(icon: ImageVector, accent: Color, busy: Boolean) {
	if (busy) {
		Box(Modifier.size(Sizes.badge), contentAlignment = Alignment.Center) { Spinner() }
	} else {
		IconBadge(icon, accent)
	}
}

/** What runs or is on its way, inside a button, a row or a tile. */
@Composable
fun Spinner(size: Dp = Sizes.iconM) = CircularProgressIndicator(Modifier.size(size), strokeWidth = Sizes.spinnerStroke)

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
