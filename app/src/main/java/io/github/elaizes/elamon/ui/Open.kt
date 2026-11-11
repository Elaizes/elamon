package io.github.elaizes.elamon.ui

import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.widget.Toast
import io.github.elaizes.elamon.R
import io.github.elaizes.elamon.data.isWebLink

/**
 * Opens [url] in the browser, or whatever handles web pages, if it is one ([isWebLink]): as a link
 * tapped on a web page would be opened (CATEGORY_BROWSABLE), and never in this app itself. Anything
 * else is refused with a word.
 */
fun openWebLink(ctx: Context, url: String) {
	val intent = Intent(Intent.ACTION_VIEW, Uri.parse(url)).addCategory(Intent.CATEGORY_BROWSABLE)
	if (!isWebLink(url) || opensHere(ctx, intent)) {
		Toast.makeText(ctx, R.string.link_refused, Toast.LENGTH_SHORT).show()
		return
	}
	runCatching { ctx.startActivity(intent) }
}

/** Whether this app is among those that would take [intent]: its own package is always seen, whatever else is. */
private fun opensHere(ctx: Context, intent: Intent): Boolean {
	val pm = ctx.packageManager
	val found = if (Build.VERSION.SDK_INT >= 33) {
		pm.queryIntentActivities(intent, PackageManager.ResolveInfoFlags.of(PackageManager.MATCH_DEFAULT_ONLY.toLong()))
	} else {
		@Suppress("DEPRECATION")
		pm.queryIntentActivities(intent, PackageManager.MATCH_DEFAULT_ONLY)
	}
	return found.any { it.activityInfo?.packageName == ctx.packageName }
}
