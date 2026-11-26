// `media`: the players, with covers and seeking, and the computer's volume. Its buttons run the
// `media.*` and `volume.set` actions, as far as the computer offers them and the player can.
import { h, setText, setStyle } from '../dom.js';
import { icon } from '../icons.js';
import { setAccent, chips } from './common.js';
import { S } from '../strings.js';
import * as F from '../format.js';

const PLAY_PAUSE = 'media.play_pause';
const NEXT = 'media.next';
const PREVIOUS = 'media.previous';
const SEEK = 'media.seek';
const VOLUME = 'volume.set';
/** How often a playing track's place moves on between lines. */
const TICK_MS = 500;
const TENTHS = 10;

/** @param {import('./common.js').Widget} w @param {import('./common.js').Env} env @returns {import('./common.js').Instance} */
export function create(w, env) {
	/** @type {import('./common.js').WidgetState | undefined} */
	let last;
	/** @type {string | null} */
	let chosen = null;
	let receivedAt = 0;
	let seeking = false;
	let draggingVolume = false;
	/** @type {ReturnType<typeof setInterval> | null} */
	let ticker = null;
	/** The cover shown: for which player and track. */
	let artKey = '';
	/** The players the chips were made for. */
	let pickersKey = '';
	/** @type {ReturnType<typeof chips> | null} */
	let picker = null;

	const title = h('h3', { class: 'w-title' });
	const head = h('div', { class: 'w-head' }, icon(w.icon || 'music', 'w-icon'), title);
	const pickers = h('div', { class: 'media-players' });
	const art = h('div', { class: 'media-art', 'aria-hidden': 'true' }, icon('music'));
	const track = h('p', { class: 'media-track' });
	const artist = h('p', { class: 'media-artist muted' });
	const nowPlaying = h('div', { class: 'media-now' }, art, h('div', { class: 'media-text' }, track, artist));
	const nothing = h('p', { class: 'muted' }, S.panels.nothingPlaying);
	const position = h('input', { type: 'range', min: '0', max: '1', step: 'any', class: 'media-seek', 'aria-label': S.panels.position });
	const at = h('span', { class: 'muted media-time' });
	const length = h('span', { class: 'muted media-time' });
	const seekRow = h('div', { class: 'media-seek-row' }, position, h('div', { class: 'media-times' }, at, length));
	const previous = h('button', { type: 'button', class: 'icon-button', 'aria-label': S.panels.previous, title: S.panels.previous }, icon('previous'));
	const play = h('button', { type: 'button', class: 'icon-button primary media-play' });
	const next = h('button', { type: 'button', class: 'icon-button', 'aria-label': S.panels.next, title: S.panels.next }, icon('next'));
	const buttons = h('div', { class: 'media-buttons' }, previous, play, next);
	const mute = h('button', { type: 'button', class: 'icon-button' });
	const volume = h('input', { type: 'range', min: '0', max: '100', step: '1', class: 'media-volume', 'aria-label': S.panels.volume });
	const volumeText = h('span', { class: 'media-volume-text' });
	const volumeRow = h('div', { class: 'media-volume-row' }, mute, volume, volumeText);
	const body = h('div', { class: 'w-body' }, pickers, nothing, nowPlaying, seekRow, buttons, volumeRow);
	const el = h('section', { class: 'card w w-media', 'aria-label': w.label || S.panels.media }, head, body);
	setAccent(el, w.color);

	/** @param {string} id */
	const action = (id) => env.action(id);
	/** @param {any} a */
	const can = (a) => !!a && env.controlOn;
	const player = () => {
		const players = last && Array.isArray(last.players) ? last.players : [];
		return players.find((p) => p.id === chosen) || players[0] || null;
	};
	/** Where a playing track is now: its place at the last line, and the time since. @param {any} p */
	const placeOf = (p) => {
		const pos = typeof p.position === 'number' ? p.position : 0;
		const moving = p.status === 'Playing' && env.live;
		const now = moving ? pos + (Date.now() - receivedAt) / 1000 : pos;
		return Math.min(Math.max(now, 0), typeof p.length === 'number' ? p.length : now);
	};

	/** @param {string} id @param {Record<string, unknown>} params @param {string} phrase */
	async function run(id, params, phrase) {
		const a = action(id);
		if (!a) return;
		await env.run(a, params, w, phrase);
	}

	previous.addEventListener('click', () => {
		const p = player();
		if (p) run(PREVIOUS, { player: p.id }, S.panels.previous);
	});
	next.addEventListener('click', () => {
		const p = player();
		if (p) run(NEXT, { player: p.id }, S.panels.next);
	});
	play.addEventListener('click', () => {
		const p = player();
		if (p) run(PLAY_PAUSE, { player: p.id }, p.status === 'Playing' ? S.panels.pause : S.panels.play);
	});
	position.addEventListener('input', () => {
		seeking = true;
		setText(at, F.clock(Number(position.value)));
	});
	position.addEventListener('change', () => {
		seeking = false;
		const p = player();
		if (p) run(SEEK, { player: p.id, position: Math.round(Number(position.value) * TENTHS) / TENTHS }, S.panels.position);
	});
	mute.addEventListener('click', () => {
		const v = last && last.volume;
		if (v) run(VOLUME, { muted: !v.muted }, v.muted ? S.panels.unmute : S.panels.mute);
	});
	volume.addEventListener('input', () => {
		draggingVolume = true;
		setText(volumeText, F.percent(Number(volume.value)));
	});
	volume.addEventListener('change', () => {
		draggingVolume = false;
		run(VOLUME, { level: Math.round(Number(volume.value)) }, S.panels.volume);
	});

	function tick() {
		const p = player();
		if (!p || seeking || typeof p.length !== 'number' || p.length <= 0) return;
		const place = placeOf(p);
		position.value = String(place);
		setText(at, F.clock(place));
	}

	function show() {
		const s = last;
		const players = s && Array.isArray(s.players) ? s.players : [];
		const p = player();
		setText(title, (p && p.name) || w.label || S.panels.media);
		const listed = players.length > 1 ? players.map((x) => `${x.id}\u0000${x.name}`).join('\u0001') : '';
		if (listed !== pickersKey) {
			pickersKey = listed;
			picker = listed
				? chips(S.panels.players, players.map((x) => ({ value: x.id, text: x.name || x.id })), p ? p.id : '', (id) => {
						chosen = id;
						show();
					})
				: null;
			pickers.replaceChildren(...(picker ? [picker.el] : []));
		} else if (picker && p) picker.select(p.id);
		nothing.hidden = !s || !!p;
		nowPlaying.hidden = !p;
		buttons.hidden = !p || !(action(PLAY_PAUSE) || action(NEXT) || action(PREVIOUS));
		if (p) {
			setText(track, p.title || (p.status === 'Stopped' ? S.panels.nothingPlaying : S.panels.unknownTrack));
			setText(artist, [p.artist, p.album].filter(Boolean).join(' · '));
			const key = p.has_art ? `${p.id}\u0000${p.title || ''}\u0000${p.album || ''}` : '';
			if (key !== artKey) {
				artKey = key;
				if (key && env.live) {
					const img = h('img', { alt: '', src: `/api/v1/media/art?player=${encodeURIComponent(p.id)}`, decoding: 'async' });
					img.addEventListener('error', () => art.replaceChildren(icon('music')));
					art.replaceChildren(img);
				} else art.replaceChildren(icon('music'));
			}
			const playing = p.status === 'Playing';
			play.replaceChildren(icon(playing ? 'pause' : 'play'));
			play.setAttribute('aria-label', playing ? S.panels.pause : S.panels.play);
			play.title = playing ? S.panels.pause : S.panels.play;
			play.disabled = !(p.can_play || p.can_pause) || !can(action(PLAY_PAUSE));
			previous.disabled = !p.can_previous || !can(action(PREVIOUS));
			next.disabled = !p.can_next || !can(action(NEXT));
			const len = typeof p.length === 'number' && p.length > 0 ? p.length : 0;
			seekRow.hidden = !len;
			if (len) {
				position.max = String(len);
				position.disabled = !p.can_seek || !can(action(SEEK));
				setText(length, F.clock(len));
				tick();
			}
		} else {
			seekRow.hidden = true;
		}
		const v = s && s.volume;
		volumeRow.hidden = !v;
		if (v) {
			const set = action(VOLUME);
			mute.replaceChildren(icon(v.muted ? 'mute' : 'volume'));
			mute.setAttribute('aria-label', v.muted ? S.panels.unmute : S.panels.mute);
			mute.title = v.muted ? S.panels.unmute : S.panels.mute;
			mute.classList.toggle('muted-on', !!v.muted);
			mute.disabled = !can(set);
			volume.disabled = !can(set);
			if (!draggingVolume) {
				volume.value = String(Math.round(v.level || 0));
				setText(volumeText, F.percent(v.level || 0));
			}
			setStyle(volume, { '--level': `${Math.min(Math.max(v.level || 0, 0), 100)}%` });
		}
		const moving = !!p && p.status === 'Playing' && env.live;
		if (moving && !ticker) ticker = setInterval(tick, TICK_MS);
		if (!moving && ticker) {
			clearInterval(ticker);
			ticker = null;
		}
	}
	show();

	return {
		el,
		update(s) {
			last = s;
			receivedAt = Date.now();
			show();
		},
		clear() {
			last = undefined;
			show();
		},
		destroy() {
			if (ticker) clearInterval(ticker);
		},
	};
}
