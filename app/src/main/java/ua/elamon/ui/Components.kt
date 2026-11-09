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
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp

/** A titled card: the building block of every tab. */
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
	Surface(
		modifier = modifier.fillMaxWidth(),
		shape = RoundedCornerShape(24.dp),
		color = MaterialTheme.colorScheme.surfaceContainerLow,
	) {
		Column(
			Modifier
				.then(if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier)
				.padding(horizontal = 18.dp, vertical = 16.dp),
			verticalArrangement = Arrangement.spacedBy(12.dp),
		) {
			Row(verticalAlignment = Alignment.CenterVertically) {
				Box(
					Modifier.size(32.dp).clip(CircleShape).background(accent.copy(alpha = 0.16f)),
					contentAlignment = Alignment.Center,
				) {
					Icon(icon, null, tint = accent, modifier = Modifier.size(18.dp))
				}
				Spacer(Modifier.width(10.dp))
				Text(
					title,
					style = MaterialTheme.typography.titleMedium,
					fontWeight = FontWeight.SemiBold,
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

/** A label above a big number, the way dashboards show their main figures. */
@Composable
fun BigStat(label: String, value: String, modifier: Modifier = Modifier, color: Color = MaterialTheme.colorScheme.onSurface) {
	Column(modifier) {
		Text(label, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1)
		Text(value, style = MaterialTheme.typography.titleLarge, color = color, fontWeight = FontWeight.SemiBold, maxLines = 1)
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
		Text(value, style = MaterialTheme.typography.bodyMedium, color = valueColor, maxLines = 1)
	}
}

/** A small rounded tag, like "Charging" or "Sleeping". */
@Composable
fun Pill(text: String, color: Color = MaterialTheme.colorScheme.primary, modifier: Modifier = Modifier) {
	Box(
		modifier.clip(RoundedCornerShape(50)).background(color.copy(alpha = 0.16f)).padding(horizontal = 10.dp, vertical = 3.dp),
	) {
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
		Icon(icon, null, modifier = Modifier.size(48.dp), tint = MaterialTheme.colorScheme.primary)
		Text(title, style = MaterialTheme.typography.titleMedium, textAlign = androidx.compose.ui.text.style.TextAlign.Center)
		if (body != null) {
			Text(
				body,
				style = MaterialTheme.typography.bodyMedium,
				color = MaterialTheme.colorScheme.onSurfaceVariant,
				textAlign = androidx.compose.ui.text.style.TextAlign.Center,
			)
		}
		action?.invoke()
	}
}
