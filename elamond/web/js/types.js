// The shapes of the protocol the page reads (docs/protocol.md), for `tsc --checkJs`. Nothing runs here.

/**
 * @typedef {{ api: number, app: string, id: string, name: string, version: string, min_app: string, widgets: number, fingerprint?: string }} Hello
 *
 * @typedef {{ id: string, name: string, version: string, interval_ms: number, history_seconds: number, features: string[], endpoints: string[] }} Service
 *
 * @typedef {{ id: string, name: string, theme: 'system' | 'light' | 'dark', pure_black: boolean, animations: boolean, interval_ms: number,
 *   fahrenheit: boolean, network_bits: boolean, confirm_actions: boolean, web?: Record<string, unknown>,
 *   source?: { file: string, line: number } }} Profile
 *
 * @typedef {{ id: string, type: string, span: number, label?: string, icon?: string, color?: string, open?: string, caption?: string | boolean,
 *   widgets?: Widget[], [key: string]: any }} Widget
 *
 * @typedef {{ id: string, title: string, icon?: string, widgets: Widget[], [key: string]: any }} Dashboard
 *
 * @typedef {{ id: string, name: string, kind: string, icon?: string, confirm: boolean, unlock: boolean, params: any[], [key: string]: any }} Action
 *
 * @typedef {{ version: string, service: Service, role: string, control: { enabled: boolean, off_reason: string | null }, profile: Profile,
 *   tabs: string[], header: string | null, dashboards: Dashboard[], actions: Action[] }} Ui
 *
 * @typedef {{ v?: any, text?: string, level?: string, blend?: number, max?: number, caption?: string, enabled?: boolean, [key: string]: any }} WidgetState
 *
 * A `data` line, or a snapshot or preview's `data`.
 * @typedef {{ t: number, dashboard: string, widgets: Record<string, WidgetState>, history?: Record<string, { t: number[], v: (number | null)[][] }> }} Data
 *
 * What a dashboard view needs besides `ui`: how to run an action, how to change a table's order or a
 * log's filter on the stream, how to open another dashboard. A preview passes none of them, and the
 * view draws its controls but runs nothing.
 * @typedef {{ runAction?: (action: Action, params: Record<string, unknown>, widget: Widget) => Promise<void>,
 *   change?: (body: Record<string, unknown>) => void, open?: (dashboard: string) => void, preview?: boolean }} ViewHooks
 *
 * A dashboard drawn: its element, the next data, and away.
 * @typedef {{ el: HTMLElement, update: (data: Data) => void, destroy: () => void }} DashboardView
 *
 * What every part of the page is handed: the service as `hello` says, the `ui` as last read, and
 * the page's ways about.
 * @typedef {{ hello: Hello, ui: () => Ui | null, reloadUi: () => Promise<Ui | null>, go: (hash: string) => void,
 *   toast: (text: string, level?: 'normal' | 'warning' | 'critical') => void, has: (feature: string) => boolean }} Ctx
 */

export {};
