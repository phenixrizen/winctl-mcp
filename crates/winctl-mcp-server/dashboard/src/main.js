import { createApp } from 'vue/dist/vue.esm-bundler.js';
import './styles.css';
import logoUrl from '../../../../assets/brand/winctl-logo.svg';

const params = new URLSearchParams(window.location.search);
const urlToken = params.get('token');
if (urlToken) {
  sessionStorage.setItem('winctl.dashboard.token', urlToken);
}
const authToken = urlToken || sessionStorage.getItem('winctl.dashboard.token');

function authHeaders() {
  return authToken ? { Authorization: `Bearer ${authToken}` } : {};
}

function formatUnixMs(value) {
  if (!value) return 'n/a';
  return new Intl.DateTimeFormat(undefined, {
    dateStyle: 'medium',
    timeStyle: 'medium',
  }).format(new Date(value));
}

function shortPath(value) {
  if (!value) return 'n/a';
  const normalized = String(value).replaceAll('\\\\', '\\');
  const parts = normalized.split(/[\\/]/).filter(Boolean);
  if (parts.length <= 3) return normalized;
  return `${parts[0]}\\...\\${parts.slice(-2).join('\\')}`;
}

function rectText(windowInfo) {
  if (!windowInfo) return 'n/a';
  return `${windowInfo.x}, ${windowInfo.y} / ${windowInfo.width} x ${windowInfo.height}`;
}

createApp({
  data() {
    return {
      logoUrl,
      data: null,
      loading: true,
      refreshing: false,
      error: null,
      selectedTab: 'overview',
      selectedBoundId: '',
      selectedDiffIndex: 0,
      uiaSnapshot: null,
      screenshotResult: null,
      inspectError: null,
      autoRefresh: true,
      lastUpdated: null,
      intervalId: null,
      tabs: [
        { id: 'overview', label: 'Overview' },
        { id: 'control', label: 'Control' },
        { id: 'inspect', label: 'Inspect' },
        { id: 'artifacts', label: 'Artifacts' },
        { id: 'catalog', label: 'Catalog' },
        { id: 'windows', label: 'Windows' },
        { id: 'processes', label: 'Processes' },
        { id: 'memory', label: 'Memory' },
        { id: 'raw', label: 'Raw' },
      ],
    };
  },
  computed: {
    boundWindows() {
      return this.data?.bound_windows ?? [];
    },
    launchedProcesses() {
      return this.data?.launched_processes ?? [];
    },
    memoryItems() {
      return this.data?.memory?.items ?? this.data?.memory?.memories ?? [];
    },
    macroItems() {
      return this.data?.macros?.items ?? this.data?.macros?.macros ?? [];
    },
    macroResults() {
      return this.data?.macro_results?.results ?? [];
    },
    visualDiffs() {
      const diffs = [];
      for (const run of this.macroResults) {
        for (const step of run.result?.step_results ?? []) {
          const output = step.output ?? {};
          const comparison = output.comparison ?? output;
          const diffPath = comparison.diff_path;
          const actualPath = output.actual_path ?? comparison.actual_path;
          const baselinePath = output.baseline_path ?? comparison.baseline_path;
          const passed = output.passed ?? comparison.passed;
          if (diffPath && actualPath && baselinePath && passed === false) {
            diffs.push({
              id: `${run.run_id}:${step.step_id}`,
              run_id: run.run_id,
              step_id: step.step_id,
              title: run.result?.manifest_title ?? 'run',
              actual_path: actualPath,
              baseline_path: baselinePath,
              diff_path: diffPath,
              different_pixels: comparison.different_pixels,
              max_different_pixels: comparison.max_different_pixels,
            });
          }
        }
      }
      return diffs;
    },
    selectedDiff() {
      return this.visualDiffs[this.selectedDiffIndex] ?? this.visualDiffs[0] ?? null;
    },
    videoArtifacts() {
      const videos = [];
      for (const run of this.macroResults) {
        for (const artifact of run.result?.artifacts ?? []) {
          if (artifact.kind === 'capture.video' && artifact.path) {
            videos.push({
              id: `${run.run_id}:${artifact.path}`,
              run_id: run.run_id,
              title: run.result?.manifest_title ?? 'run',
              path: artifact.path,
              metadata: artifact.metadata ?? {},
            });
          }
        }
      }
      return videos;
    },
    controlState() {
      return this.data?.control ?? {};
    },
    controlEvents() {
      return this.controlState.events ?? [];
    },
    warnings() {
      return this.data?.warnings ?? [];
    },
    policyFlags() {
      const policy = this.data?.policy ?? {};
      return [
        ['Filesystem mutation', policy.enable_filesystem_mutation],
        ['Clipboard write', policy.enable_clipboard_write],
        ['Registry mutation', policy.enable_registry_mutation],
        ['Private network', policy.allow_private_network],
        ['Memory mutation', policy.memory_mutation_enabled],
        ['Macro execution', policy.macro_execution_enabled],
        ['Destructive macros', policy.macro_destructive_tools_allowed],
      ];
    },
    serverSummary() {
      return {
        service: this.data?.service ?? 'winctl-mcp-server',
        version: this.data?.version ?? 'n/a',
        capture_dir: this.data?.capture_dir ?? 'n/a',
        connected_clients: this.data?.connected_clients ?? null,
        recent_requests: this.data?.recent_requests ?? [],
      };
    },
    statusLabel() {
      if (this.error) return 'Attention';
      if (this.refreshing) return 'Refreshing';
      if (this.data) return 'Healthy';
      return 'Loading';
    },
    statusColor() {
      if (this.error) return 'bg-[#ff6b6b]';
      if (this.controlState.status === 'revoked' || this.controlState.status === 'blocked') return 'bg-[#ff6b6b]';
      if (this.controlState.status === 'controlling' || this.controlState.status === 'warning') return 'bg-[#ffd166]';
      if (this.refreshing) return 'bg-[#ffd166]';
      return 'bg-[#06d6a0]';
    },
    activeBoundId() {
      return this.selectedBoundId || this.boundWindows[0]?.bound_id || '';
    },
    screenshotImageUrl() {
      const path = this.screenshotResult?.screenshot?.output_path;
      if (!path) return '';
      return this.captureFileUrl(path);
    },
    controlBadgeClass() {
      const status = this.controlState.status;
      if (status === 'revoked' || status === 'blocked') return 'badge-error';
      if (status === 'controlling' || status === 'warning') return 'badge-warning';
      if (status === 'armed') return 'badge-success';
      return 'badge-ghost';
    },
  },
  mounted() {
    this.loadState();
    this.intervalId = window.setInterval(() => {
      if (this.autoRefresh) this.loadState({ quiet: true });
    }, 5000);
  },
  beforeUnmount() {
    window.clearInterval(this.intervalId);
  },
  methods: {
    async loadState(options = {}) {
      this.error = null;
      this.refreshing = true;
      if (!options.quiet) this.loading = true;
      try {
        const response = await fetch('/dashboard/state', {
          cache: 'no-store',
          headers: authHeaders(),
        });
        if (!response.ok) throw new Error(`HTTP ${response.status}`);
        this.data = await response.json();
        this.lastUpdated = new Date();
      } catch (error) {
        this.error = String(error);
      } finally {
        this.loading = false;
        this.refreshing = false;
      }
    },
    pretty(value) {
      return JSON.stringify(value, null, 2);
    },
    formatUnixMs,
    shortPath,
    rectText,
    captureFileUrl(path) {
      if (!path) return '';
      const tokenQuery = authToken ? `&token=${encodeURIComponent(authToken)}` : '';
      return `/dashboard/capture-file?path=${encodeURIComponent(path)}${tokenQuery}`;
    },
    processLabel(process) {
      return process.process_name || process.exe || process.executable_path || 'process';
    },
    manifestTitle(item) {
      return item.manifest?.title || item.title || item.name || item.id || 'Untitled manifest';
    },
    manifestDescription(item) {
      return item.manifest?.description || item.description || item.text || '';
    },
    async copyJson(value) {
      await navigator.clipboard?.writeText(JSON.stringify(value, null, 2)).catch(() => {});
    },
    boundTitle(bound) {
      return bound.window?.title || bound.title_at_bind || 'Untitled window';
    },
    badgeClass(value) {
      return value ? 'badge-success' : 'badge-ghost';
    },
    async loadUiaSnapshot() {
      if (!this.activeBoundId) return;
      this.inspectError = null;
      try {
        const response = await fetch(`/dashboard/uia?bound_id=${encodeURIComponent(this.activeBoundId)}`, {
          cache: 'no-store',
          headers: authHeaders(),
        });
        if (!response.ok) throw new Error(`HTTP ${response.status}`);
        this.uiaSnapshot = await response.json();
      } catch (error) {
        this.inspectError = String(error);
      }
    },
    async captureScreenshot() {
      if (!this.activeBoundId) return;
      this.inspectError = null;
      try {
        const response = await fetch(`/dashboard/screenshot?bound_id=${encodeURIComponent(this.activeBoundId)}`, {
          cache: 'no-store',
          headers: authHeaders(),
        });
        if (!response.ok) throw new Error(`HTTP ${response.status}`);
        this.screenshotResult = await response.json();
      } catch (error) {
        this.inspectError = String(error);
      }
    },
  },
  template: `
    <div class="winctl-shell">
      <header class="winctl-topbar">
        <div class="mx-auto flex max-w-7xl flex-col gap-5 px-4 py-5 sm:px-6 lg:px-8">
          <div class="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
            <div class="flex items-center gap-4">
              <img :src="logoUrl" alt="winctl" class="winctl-logo h-12 w-12 rounded-xl" />
              <div>
                <h1 class="text-xl font-semibold leading-tight">winctl-mcp dashboard</h1>
                <div class="mt-1 flex flex-wrap items-center gap-2 text-sm text-slate-300">
                  <span class="inline-flex items-center gap-2">
                    <span class="winctl-dot" :class="statusColor"></span>
                    {{ statusLabel }}
                  </span>
                  <span v-if="lastUpdated">Updated {{ lastUpdated.toLocaleTimeString() }}</span>
                  <span v-if="data">v{{ data.version }}</span>
                </div>
              </div>
            </div>
            <div class="flex flex-wrap items-center gap-2">
              <label class="label cursor-pointer gap-2 rounded-md border border-white/15 px-3 py-2 text-sm text-slate-200">
                <input v-model="autoRefresh" type="checkbox" class="toggle toggle-info toggle-sm" />
                <span>Auto</span>
              </label>
              <button type="button" class="btn btn-sm border-white/20 bg-white/10 text-white hover:bg-white/20" @click="loadState()">
                Refresh
              </button>
            </div>
          </div>
          <div v-if="error" class="alert border-[#ff6b6b]/40 bg-[#ff6b6b]/12 text-white">
            <span>{{ error }}</span>
          </div>
        </div>
      </header>

      <main class="mx-auto max-w-7xl px-4 py-5 sm:px-6 lg:px-8">
        <nav class="mb-5 flex overflow-x-auto border-b border-slate-200 dark:border-slate-700" aria-label="Dashboard sections">
          <button
            v-for="tab in tabs"
            :key="tab.id"
            type="button"
            class="winctl-tab shrink-0 px-4 py-3 text-sm font-semibold"
            :aria-selected="selectedTab === tab.id"
            @click="selectedTab = tab.id"
          >
            {{ tab.label }}
          </button>
        </nav>

        <section v-if="selectedTab === 'overview'" class="space-y-5">
          <div class="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
            <article class="winctl-card p-4">
              <div class="winctl-kpi-accent mb-4"></div>
              <div class="text-xs font-bold uppercase text-slate-500">Bound windows</div>
              <div class="mt-2 text-3xl font-semibold">{{ boundWindows.length }}</div>
            </article>
            <article class="winctl-card p-4">
              <div class="winctl-kpi-accent mb-4"></div>
              <div class="text-xs font-bold uppercase text-slate-500">Launched processes</div>
              <div class="mt-2 text-3xl font-semibold">{{ launchedProcesses.length }}</div>
            </article>
            <article class="winctl-card p-4">
              <div class="winctl-kpi-accent mb-4"></div>
              <div class="text-xs font-bold uppercase text-slate-500">Memory items</div>
              <div class="mt-2 text-3xl font-semibold">{{ memoryItems.length }}</div>
            </article>
            <article class="winctl-card p-4">
              <div class="winctl-kpi-accent mb-4"></div>
              <div class="text-xs font-bold uppercase text-slate-500">Macros</div>
              <div class="mt-2 text-3xl font-semibold">{{ macroItems.length }}</div>
            </article>
            <article class="winctl-card p-4">
              <div class="winctl-kpi-accent mb-4"></div>
              <div class="text-xs font-bold uppercase text-slate-500">Control</div>
              <div class="mt-3">
                <span class="badge" :class="controlBadgeClass">{{ controlState.status || 'idle' }}</span>
              </div>
            </article>
          </div>

          <div class="grid gap-5 lg:grid-cols-[1fr_1.15fr]">
            <section class="winctl-card">
              <div class="winctl-card-header px-4 py-3">
                <h2 class="text-sm font-semibold">Server</h2>
              </div>
              <dl class="divide-y divide-slate-200 text-sm dark:divide-slate-700">
                <div class="grid grid-cols-[140px_1fr] gap-3 px-4 py-3">
                  <dt class="text-slate-500">Service</dt>
                  <dd class="font-medium">{{ serverSummary.service }}</dd>
                </div>
                <div class="grid grid-cols-[140px_1fr] gap-3 px-4 py-3">
                  <dt class="text-slate-500">Version</dt>
                  <dd class="font-medium">{{ serverSummary.version }}</dd>
                </div>
                <div class="grid grid-cols-[140px_1fr] gap-3 px-4 py-3">
                  <dt class="text-slate-500">Capture dir</dt>
                  <dd class="winctl-code break-all text-xs">{{ serverSummary.capture_dir }}</dd>
                </div>
              </dl>
            </section>

            <section class="winctl-card">
              <div class="winctl-card-header px-4 py-3">
                <h2 class="text-sm font-semibold">Policy</h2>
              </div>
              <div class="grid gap-2 p-4 sm:grid-cols-2">
                <div v-for="[label, enabled] in policyFlags" :key="label" class="flex items-center justify-between gap-3 rounded-md border border-slate-200 px-3 py-2 text-sm dark:border-slate-700">
                  <span>{{ label }}</span>
                  <span class="badge badge-sm" :class="badgeClass(enabled)">{{ enabled ? 'On' : 'Off' }}</span>
                </div>
              </div>
            </section>
          </div>

          <section v-if="warnings.length" class="winctl-card">
            <div class="winctl-card-header px-4 py-3">
              <h2 class="text-sm font-semibold">Warnings</h2>
            </div>
            <ul class="divide-y divide-slate-200 text-sm dark:divide-slate-700">
              <li v-for="warning in warnings" :key="warning" class="px-4 py-3">{{ warning }}</li>
            </ul>
          </section>
        </section>

        <section v-if="selectedTab === 'control'" class="grid gap-5 lg:grid-cols-[0.85fr_1.15fr]">
          <section class="winctl-card">
            <div class="winctl-card-header flex items-center justify-between gap-3 px-4 py-3">
              <h2 class="text-sm font-semibold">Control gate</h2>
              <span class="badge" :class="controlBadgeClass">{{ controlState.status || 'idle' }}</span>
            </div>
            <dl class="divide-y divide-slate-200 text-sm dark:divide-slate-700">
              <div class="grid grid-cols-[150px_1fr] gap-3 px-4 py-3">
                <dt class="text-slate-500">Session</dt>
                <dd class="winctl-code break-all text-xs">{{ controlState.session_id || 'n/a' }}</dd>
              </div>
              <div class="grid grid-cols-[150px_1fr] gap-3 px-4 py-3">
                <dt class="text-slate-500">Bound target</dt>
                <dd class="winctl-code break-all text-xs">{{ controlState.bound_id || 'n/a' }}</dd>
              </div>
              <div class="grid grid-cols-[150px_1fr] gap-3 px-4 py-3">
                <dt class="text-slate-500">Active tool</dt>
                <dd class="winctl-code text-xs">{{ controlState.active_tool || 'n/a' }}</dd>
              </div>
              <div class="grid grid-cols-[150px_1fr] gap-3 px-4 py-3">
                <dt class="text-slate-500">Action</dt>
                <dd class="winctl-code text-xs">{{ controlState.active_action_kind || 'n/a' }}</dd>
              </div>
              <div class="grid grid-cols-[150px_1fr] gap-3 px-4 py-3">
                <dt class="text-slate-500">Armed until</dt>
                <dd>{{ formatUnixMs(controlState.armed_until_unix_ms) }}</dd>
              </div>
              <div class="grid grid-cols-[150px_1fr] gap-3 px-4 py-3">
                <dt class="text-slate-500">Emergency stop</dt>
                <dd>
                  <span class="badge badge-sm" :class="badgeClass(controlState.emergency_stop_active)">
                    {{ controlState.emergency_stop_active ? 'Active' : 'Clear' }}
                  </span>
                </dd>
              </div>
            </dl>
          </section>

          <section class="winctl-card overflow-hidden">
            <div class="winctl-card-header flex items-center justify-between gap-3 px-4 py-3">
              <h2 class="text-sm font-semibold">Recent control events</h2>
              <span class="badge badge-info badge-outline">{{ controlEvents.length }}</span>
            </div>
            <div class="overflow-x-auto">
              <table class="table winctl-table table-sm min-w-[880px]">
                <thead>
                  <tr>
                    <th>Time</th>
                    <th>Kind</th>
                    <th>Status</th>
                    <th>Tool</th>
                    <th>Target</th>
                    <th>Message</th>
                  </tr>
                </thead>
                <tbody>
                  <tr v-if="!controlEvents.length">
                    <td colspan="6" class="py-8 text-center text-slate-500">No control events</td>
                  </tr>
                  <tr v-for="event in controlEvents.slice().reverse()" :key="event.id">
                    <td class="text-xs">{{ formatUnixMs(event.timestamp_unix_ms) }}</td>
                    <td class="winctl-code text-xs">{{ event.kind }}</td>
                    <td><span class="badge badge-ghost badge-sm">{{ event.status }}</span></td>
                    <td class="winctl-code text-xs">{{ event.tool_name || 'n/a' }}</td>
                    <td class="winctl-code max-w-[220px] truncate text-xs">{{ event.bound_id || 'n/a' }}</td>
                    <td class="max-w-[360px] truncate text-xs">{{ event.message }}</td>
                  </tr>
                </tbody>
              </table>
            </div>
          </section>
        </section>

        <section v-if="selectedTab === 'inspect'" class="grid gap-5 lg:grid-cols-[0.9fr_1.1fr]">
          <section class="winctl-card">
            <div class="winctl-card-header flex items-center justify-between gap-3 px-4 py-3">
              <h2 class="text-sm font-semibold">Active binding</h2>
              <span class="badge badge-info badge-outline">{{ boundWindows.length }}</span>
            </div>
            <div class="space-y-4 p-4">
              <select v-model="selectedBoundId" class="select select-bordered w-full">
                <option value="">First bound window</option>
                <option v-for="bound in boundWindows" :key="bound.bound_id" :value="bound.bound_id">
                  {{ boundTitle(bound) }} - {{ bound.bound_id }}
                </option>
              </select>
              <div class="flex flex-wrap gap-2">
                <button type="button" class="btn btn-sm btn-info" :disabled="!activeBoundId" @click="loadUiaSnapshot">
                  UIA snapshot
                </button>
                <button type="button" class="btn btn-sm" :disabled="!activeBoundId" @click="captureScreenshot">
                  Screenshot
                </button>
              </div>
              <div v-if="inspectError" class="alert alert-error text-sm">{{ inspectError }}</div>
              <div v-if="screenshotResult?.screenshot" class="space-y-3">
                <img v-if="screenshotImageUrl" :src="screenshotImageUrl" alt="Bound window screenshot" class="winctl-screenshot" />
                <pre class="winctl-json m-0 p-3 text-xs">{{ pretty(screenshotResult.screenshot) }}</pre>
              </div>
            </div>
          </section>

          <section class="winctl-card overflow-hidden">
            <div class="winctl-card-header px-4 py-3">
              <h2 class="text-sm font-semibold">UI Automation tree</h2>
            </div>
            <pre class="winctl-json m-0 p-4 text-xs">{{ pretty(uiaSnapshot ?? {}) }}</pre>
          </section>
        </section>

        <section v-if="selectedTab === 'artifacts'" class="space-y-5">
          <section class="winctl-card">
            <div class="winctl-card-header flex items-center justify-between gap-3 px-4 py-3">
              <h2 class="text-sm font-semibold">Visual diffs</h2>
              <span class="badge badge-info badge-outline">{{ visualDiffs.length }}</span>
            </div>
            <div v-if="!visualDiffs.length" class="px-4 py-8 text-center text-sm text-slate-500">No failed visual baseline artifacts</div>
            <div v-else class="grid gap-4 p-4 lg:grid-cols-[320px_1fr]">
              <div class="space-y-2">
                <button
                  v-for="(diff, index) in visualDiffs"
                  :key="diff.id"
                  type="button"
                  class="winctl-diff-row w-full text-left"
                  :aria-selected="selectedDiffIndex === index"
                  @click="selectedDiffIndex = index"
                >
                  <div class="font-medium">{{ diff.title }}</div>
                  <div class="winctl-code truncate text-xs text-slate-500">{{ diff.step_id }} · {{ diff.different_pixels }} px</div>
                </button>
              </div>
              <div v-if="selectedDiff" class="space-y-4">
                <div class="grid gap-4 lg:grid-cols-3">
                  <figure class="winctl-artifact-panel">
                    <figcaption>Baseline</figcaption>
                    <img :src="captureFileUrl(selectedDiff.baseline_path)" alt="Baseline artifact" />
                  </figure>
                  <figure class="winctl-artifact-panel">
                    <figcaption>Actual</figcaption>
                    <img :src="captureFileUrl(selectedDiff.actual_path)" alt="Actual artifact" />
                  </figure>
                  <figure class="winctl-artifact-panel">
                    <figcaption>Diff</figcaption>
                    <img :src="captureFileUrl(selectedDiff.diff_path)" alt="Diff artifact" />
                  </figure>
                </div>
                <pre class="winctl-json m-0 p-3 text-xs">{{ pretty(selectedDiff) }}</pre>
              </div>
            </div>
          </section>

          <section class="winctl-card">
            <div class="winctl-card-header flex items-center justify-between gap-3 px-4 py-3">
              <h2 class="text-sm font-semibold">Run videos</h2>
              <span class="badge badge-info badge-outline">{{ videoArtifacts.length }}</span>
            </div>
            <div v-if="!videoArtifacts.length" class="px-4 py-8 text-center text-sm text-slate-500">No run-video artifacts</div>
            <div v-else class="grid gap-4 p-4 lg:grid-cols-2">
              <figure v-for="video in videoArtifacts" :key="video.id" class="winctl-artifact-panel">
                <figcaption>{{ video.title }}</figcaption>
                <img :src="captureFileUrl(video.path)" alt="Run video artifact" />
                <pre class="mt-3 max-h-48 overflow-auto text-xs">{{ pretty(video.metadata) }}</pre>
              </figure>
            </div>
          </section>

          <section class="winctl-card overflow-hidden">
            <div class="winctl-card-header flex items-center justify-between gap-3 px-4 py-3">
              <h2 class="text-sm font-semibold">Completions</h2>
              <span class="badge badge-info badge-outline">{{ macroResults.length }}</span>
            </div>
            <div class="overflow-x-auto">
              <table class="table winctl-table table-sm min-w-[760px]">
                <thead>
                  <tr><th>Run</th><th>Status</th><th>Finished</th><th>Artifacts</th></tr>
                </thead>
                <tbody>
                  <tr v-if="!macroResults.length">
                    <td colspan="4" class="py-8 text-center text-slate-500">No completed macro or test runs</td>
                  </tr>
                  <tr v-for="run in macroResults" :key="run.run_id">
                    <td>
                      <div class="font-medium">{{ run.result?.manifest_title || run.run_id }}</div>
                      <div class="winctl-code text-xs text-slate-500">{{ run.run_id }}</div>
                    </td>
                    <td><span class="badge badge-sm" :class="run.result?.status === 'succeeded' ? 'badge-success' : 'badge-error'">{{ run.result?.status }}</span></td>
                    <td class="text-xs">{{ run.result?.finished_at || 'n/a' }}</td>
                    <td class="text-xs">{{ (run.result?.artifacts ?? []).length }}</td>
                  </tr>
                </tbody>
              </table>
            </div>
          </section>
        </section>

        <section v-if="selectedTab === 'catalog'" class="winctl-card overflow-hidden">
          <div class="winctl-card-header flex items-center justify-between gap-3 px-4 py-3">
            <h2 class="text-sm font-semibold">Manifest catalog</h2>
            <span class="badge badge-info badge-outline">{{ macroItems.length }}</span>
          </div>
          <div class="grid gap-3 p-4 lg:grid-cols-2">
            <article v-if="!macroItems.length" class="py-8 text-center text-sm text-slate-500 lg:col-span-2">No saved macros or test manifests</article>
            <article v-for="item in macroItems" :key="item.id || manifestTitle(item)" class="winctl-catalog-item">
              <div class="flex items-start justify-between gap-3">
                <div>
                  <h3 class="text-sm font-semibold">{{ manifestTitle(item) }}</h3>
                  <p class="mt-1 text-xs text-slate-500">{{ manifestDescription(item) }}</p>
                </div>
                <button type="button" class="btn btn-xs" @click="copyJson({ manifest: item.manifest ?? item })">Copy run JSON</button>
              </div>
              <div class="mt-3 flex flex-wrap gap-1">
                <span class="badge badge-ghost badge-sm">{{ item.kind || item.manifest?.version || 'manifest' }}</span>
                <span v-for="tag in item.tags ?? item.manifest?.tags ?? []" :key="tag" class="badge badge-outline badge-sm">{{ tag }}</span>
              </div>
            </article>
          </div>
        </section>

        <section v-if="selectedTab === 'windows'" class="winctl-card overflow-hidden">
          <div class="winctl-card-header flex items-center justify-between gap-3 px-4 py-3">
            <h2 class="text-sm font-semibold">Bound windows</h2>
            <span class="badge badge-info badge-outline">{{ boundWindows.length }}</span>
          </div>
          <div class="overflow-x-auto">
            <table class="table winctl-table table-sm min-w-[920px]">
              <thead>
                <tr>
                  <th>Title</th>
                  <th>HWND</th>
                  <th>PID</th>
                  <th>Process</th>
                  <th>Rect</th>
                  <th>State</th>
                </tr>
              </thead>
              <tbody>
                <tr v-if="!boundWindows.length">
                  <td colspan="6" class="py-8 text-center text-slate-500">No bound windows</td>
                </tr>
                <tr v-for="bound in boundWindows" :key="bound.bound_id">
                  <td>
                    <div class="max-w-[320px] truncate font-medium">{{ boundTitle(bound) }}</div>
                    <div class="winctl-code max-w-[320px] truncate text-xs text-slate-500">{{ bound.bound_id }}</div>
                  </td>
                  <td class="winctl-code text-xs">{{ bound.identity?.hwnd_hex }}</td>
                  <td class="winctl-code text-xs">{{ bound.identity?.pid }}</td>
                  <td>
                    <div>{{ bound.window?.process_name || 'n/a' }}</div>
                    <div class="max-w-[260px] truncate text-xs text-slate-500">{{ shortPath(bound.window?.exe_path) }}</div>
                  </td>
                  <td class="winctl-code text-xs">{{ rectText(bound.window) }}</td>
                  <td>
                    <div class="flex flex-wrap gap-1">
                      <span v-if="bound.window?.visible" class="badge badge-success badge-outline badge-sm">visible</span>
                      <span v-if="bound.window?.foreground" class="badge badge-info badge-outline badge-sm">foreground</span>
                      <span v-if="bound.window?.minimized" class="badge badge-warning badge-outline badge-sm">minimized</span>
                      <span v-if="bound.window?.cloaked" class="badge badge-error badge-outline badge-sm">cloaked</span>
                    </div>
                  </td>
                </tr>
              </tbody>
            </table>
          </div>
        </section>

        <section v-if="selectedTab === 'processes'" class="winctl-card overflow-hidden">
          <div class="winctl-card-header flex items-center justify-between gap-3 px-4 py-3">
            <h2 class="text-sm font-semibold">Launched processes</h2>
            <span class="badge badge-info badge-outline">{{ launchedProcesses.length }}</span>
          </div>
          <div class="overflow-x-auto">
            <table class="table winctl-table table-sm min-w-[880px]">
              <thead>
                <tr>
                  <th>Process</th>
                  <th>PID</th>
                  <th>Launch ID</th>
                  <th>Started</th>
                  <th>Command</th>
                </tr>
              </thead>
              <tbody>
                <tr v-if="!launchedProcesses.length">
                  <td colspan="5" class="py-8 text-center text-slate-500">No launched processes</td>
                </tr>
                <tr v-for="process in launchedProcesses" :key="process.launch_id">
                  <td>
                    <div class="font-medium">{{ processLabel(process) }}</div>
                    <div class="max-w-[280px] truncate text-xs text-slate-500">{{ shortPath(process.executable_path) }}</div>
                  </td>
                  <td class="winctl-code text-xs">{{ process.pid }}</td>
                  <td class="winctl-code text-xs">{{ process.launch_id }}</td>
                  <td class="text-xs">{{ formatUnixMs(process.launch_time_unix_ms) }}</td>
                  <td class="winctl-code max-w-[340px] truncate text-xs">{{ process.command_line }}</td>
                </tr>
              </tbody>
            </table>
          </div>
        </section>

        <section v-if="selectedTab === 'memory'" class="grid gap-5 lg:grid-cols-2">
          <section class="winctl-card">
            <div class="winctl-card-header flex items-center justify-between gap-3 px-4 py-3">
              <h2 class="text-sm font-semibold">Memory</h2>
              <span class="badge badge-info badge-outline">{{ memoryItems.length }}</span>
            </div>
            <pre class="m-0 max-h-[560px] overflow-auto p-4 text-xs">{{ pretty(data?.memory ?? {}) }}</pre>
          </section>
          <section class="winctl-card">
            <div class="winctl-card-header flex items-center justify-between gap-3 px-4 py-3">
              <h2 class="text-sm font-semibold">Macros</h2>
              <span class="badge badge-info badge-outline">{{ macroItems.length }}</span>
            </div>
            <pre class="m-0 max-h-[560px] overflow-auto p-4 text-xs">{{ pretty(data?.macros ?? {}) }}</pre>
          </section>
        </section>

        <section v-if="selectedTab === 'raw'" class="winctl-card overflow-hidden">
          <div class="winctl-card-header px-4 py-3">
            <h2 class="text-sm font-semibold">Dashboard state</h2>
          </div>
          <pre class="winctl-json m-0 p-4 text-xs">{{ pretty(data ?? {}) }}</pre>
        </section>
      </main>
    </div>
  `,
}).mount('#app');
