import { createApp, markRaw } from 'vue/dist/vue.esm-bundler.js';
import editorWorker from 'monaco-editor/esm/vs/editor/editor.worker?worker';
import jsonWorker from 'monaco-editor/esm/vs/language/json/json.worker?worker';
import './styles.css';
import logoUrl from '../../../../assets/brand/winctl-logo.svg';

// Mermaid is heavy and only needed when a doc actually renders a diagram, so it is
// lazy-loaded (code-split) on first use rather than bundled into the main entry.
let mermaidSeq = 0;
let mermaidPromise = null;
function getMermaid() {
  if (!mermaidPromise) {
    mermaidPromise = import('mermaid').then(({ default: mermaid }) => {
      const prefersDark =
        window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches;
      mermaid.initialize({
        startOnLoad: false,
        securityLevel: 'strict',
        theme: 'base',
        themeVariables: {
          background: prefersDark ? '#181c22' : '#ffffff',
          primaryColor: prefersDark ? '#1f2630' : '#f8fafc',
          primaryTextColor: prefersDark ? '#e9edf2' : '#0b1020',
          primaryBorderColor: '#4cc9f0',
          secondaryColor: prefersDark ? '#15191f' : '#eef2f6',
          secondaryTextColor: prefersDark ? '#e9edf2' : '#0b1020',
          secondaryBorderColor: '#06d6a0',
          tertiaryColor: prefersDark ? '#202731' : '#fff8e8',
          tertiaryTextColor: prefersDark ? '#e9edf2' : '#0b1020',
          tertiaryBorderColor: '#ffb454',
          lineColor: prefersDark ? '#aeb7c2' : '#5f6b7a',
          textColor: prefersDark ? '#e9edf2' : '#0b1020',
          mainBkg: prefersDark ? '#1f2630' : '#f8fafc',
          secondBkg: prefersDark ? '#15191f' : '#eef2f6',
          nodeBorder: '#4cc9f0',
          clusterBkg: prefersDark ? '#15191f' : '#fafbfc',
          clusterBorder: prefersDark ? '#303741' : '#d9dde3',
          edgeLabelBackground: prefersDark ? '#181c22' : '#ffffff',
          actorBkg: prefersDark ? '#1f2630' : '#f8fafc',
          actorBorder: '#4cc9f0',
          actorTextColor: prefersDark ? '#e9edf2' : '#0b1020',
          labelBoxBkgColor: prefersDark ? '#15191f' : '#ffffff',
          labelBoxBorderColor: '#d9dde3',
          labelTextColor: prefersDark ? '#e9edf2' : '#0b1020',
          signalColor: prefersDark ? '#e9edf2' : '#0b1020',
          signalTextColor: prefersDark ? '#e9edf2' : '#0b1020',
          noteBkgColor: prefersDark ? '#312815' : '#fff8e8',
          noteBorderColor: '#ffb454',
          noteTextColor: prefersDark ? '#e9edf2' : '#0b1020',
        },
      });
      return mermaid;
    });
  }
  return mermaidPromise;
}

globalThis.MonacoEnvironment = {
  getWorker(_workerId, label) {
    if (label === 'json') return new jsonWorker();
    return new editorWorker();
  },
};

let monacoPromise = null;
function getMonaco() {
  if (!monacoPromise) {
    monacoPromise = import('monaco-editor/esm/vs/editor/editor.api');
  }
  return monacoPromise;
}

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

function fieldValue(row, path) {
  return path.split('.').reduce((value, part) => value?.[part], row);
}

function searchableText(value) {
  if (value == null) return '';
  if (Array.isArray(value)) return value.map(searchableText).join(' ');
  if (typeof value === 'object') return Object.values(value).map(searchableText).join(' ');
  return String(value);
}

function compactValue(value) {
  if (value == null || value === '') return 'n/a';
  if (Array.isArray(value)) return value.length ? `${value.length} items` : 'none';
  if (typeof value === 'object') return `${Object.keys(value).length} fields`;
  return String(value);
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
      docs: [],
      activeDocSlug: null,
      docsLoaded: false,
      docsLoading: false,
      docsError: null,
      docsSearch: '',
      expandedItems: {},
      deletingId: null,
      rawEditor: null,
      rawMonaco: null,
      rawEditorLoading: false,
      rawEditorError: null,
      rawEditorMaximized: false,
      tableState: {
        control: { query: '', page: 1, pageSize: 10 },
        catalog: { query: '', page: 1, pageSize: 12 },
        completions: { query: '', page: 1, pageSize: 10 },
        windows: { query: '', page: 1, pageSize: 10 },
        processes: { query: '', page: 1, pageSize: 10 },
        uia: { query: '', page: 1, pageSize: 12 },
        memory: { query: '', page: 1, pageSize: 8 },
        macros: { query: '', page: 1, pageSize: 8 },
      },
      tabs: [
        { id: 'overview', label: 'Overview' },
        { id: 'control', label: 'Control' },
        { id: 'inspect', label: 'Inspect' },
        { id: 'artifacts', label: 'Artifacts' },
        { id: 'catalog', label: 'Catalog' },
        { id: 'windows', label: 'Windows' },
        { id: 'processes', label: 'Processes' },
        { id: 'memory', label: 'Memory' },
        { id: 'docs', label: 'Docs' },
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
      const session = (this.data?.macros?.session_macros ?? []).map((macro) => ({
        key: `session:${macro.id}`,
        deleteId: macro.memory_id ?? null,
        title: macro.title || macro.manifest?.title || macro.id,
        description: macro.manifest?.description ?? '',
        kind: macro.kind || macro.manifest?.kind || 'macro',
        tags: macro.tags ?? macro.manifest?.tags ?? [],
        steps: macro.manifest?.steps?.length ?? null,
        source: 'session',
        raw: macro,
      }));
      const stored = (this.data?.macros?.memory_items ?? []).map((item) => ({
        key: `memory:${item.id}`,
        deleteId: item.id,
        title: item.title || item.id,
        description: item.text ?? '',
        kind: item.kind || 'macro',
        tags: item.tags ?? [],
        steps: item.manifest_json?.steps?.length ?? null,
        source: 'memory',
        raw: item,
      }));
      return [...session, ...stored];
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
    controlEventRows() {
      return this.controlEvents.slice().reverse();
    },
    controlTableFields() {
      return ['kind', 'status', 'tool_name', 'bound_id', 'message'];
    },
    catalogCollectionFields() {
      return ['key', 'title', 'description', 'kind', 'source', 'tags', 'raw.id', 'raw.text'];
    },
    memoryCollectionFields() {
      return ['id', 'kind', 'title', 'text', 'tags', 'created_at', 'updated_at'];
    },
    macroCollectionFields() {
      return ['key', 'title', 'description', 'kind', 'source', 'tags', 'raw.id', 'raw.text'];
    },
    completionTableFields() {
      return ['run_id', 'result.manifest_title', 'result.status', 'result.finished_at'];
    },
    windowsTableFields() {
      return [
        'bound_id',
        'title_at_bind',
        'identity.hwnd_hex',
        'identity.pid',
        'window.title',
        'window.process_name',
        'window.exe_path',
      ];
    },
    processTableFields() {
      return ['process_name', 'exe', 'executable_path', 'pid', 'launch_id', 'command_line'];
    },
    uiaTableFields() {
      return ['name', 'role', 'automation_id', 'class_name', 'element_ref'];
    },
    uiaElements() {
      const root = this.uiaSnapshot?.snapshot?.root;
      if (!root) return [];
      const rows = [];
      this.flattenUiElement(root, rows);
      return rows;
    },
    screenshotDetails() {
      const screenshot = this.screenshotResult?.screenshot;
      if (!screenshot) return [];
      return [
        ['Artifact', screenshot.artifact_id],
        ['Output path', screenshot.output_path],
        ['Region', this.rectText(screenshot.region)],
        ['Coordinate space', screenshot.coordinate_space],
        ['Captured at', screenshot.captured_at],
      ].filter(([, value]) => value != null && value !== '');
    },
    selectedDiffDetails() {
      const diff = this.selectedDiff;
      if (!diff) return [];
      return [
        ['Run', diff.run_id],
        ['Step', diff.step_id],
        ['Different pixels', diff.different_pixels],
        ['Allowed pixels', diff.max_different_pixels],
        ['Baseline', diff.baseline_path],
        ['Actual', diff.actual_path],
        ['Diff', diff.diff_path],
      ];
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
    filteredDocs() {
      const query = this.docsSearch.trim().toLowerCase();
      if (!query) return this.docs;
      return this.docs.filter(
        (doc) =>
          doc.title.toLowerCase().includes(query) ||
          doc.slug.toLowerCase().includes(query),
      );
    },
    activeDoc() {
      return this.docs.find((doc) => doc.slug === this.activeDocSlug) ?? null;
    },
    rawJson() {
      return JSON.stringify(this.data ?? {}, null, 2);
    },
  },
  watch: {
    async selectedTab(tab) {
      if (tab === 'docs') {
        await this.loadDocs();
        await this.renderMermaid();
      }
      if (tab === 'raw') this.renderRawEditor();
    },
    data() {
      this.updateRawEditor();
    },
    rawEditorMaximized() {
      this.layoutRawEditor();
    },
  },
  mounted() {
    this.loadState();
    window.addEventListener('resize', this.layoutRawEditor);
    this.intervalId = window.setInterval(() => {
      if (this.autoRefresh) this.loadState({ quiet: true });
    }, 5000);
  },
  beforeUnmount() {
    window.clearInterval(this.intervalId);
    window.removeEventListener('resize', this.layoutRawEditor);
    this.rawEditor?.dispose();
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
    async renderRawEditor() {
      await this.$nextTick();
      const host = this.$refs.rawEditor;
      if (!host) return;
      if (this.rawEditor) {
        this.updateRawEditor();
        return;
      }
      this.rawEditorLoading = true;
      this.rawEditorError = null;
      try {
        const monaco = markRaw(await getMonaco());
        const prefersDark =
          window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches;
        monaco.editor.setTheme(prefersDark ? 'vs-dark' : 'vs');
        this.rawMonaco = monaco;
        this.rawEditor = markRaw(monaco.editor.create(host, {
          value: this.rawJson,
          language: 'json',
          readOnly: true,
          automaticLayout: false,
          fontSize: 12,
          fontFamily: 'Consolas, "SFMono-Regular", ui-monospace, monospace',
          minimap: { enabled: true },
          scrollBeyondLastLine: false,
          wordWrap: 'on',
          wrappingIndent: 'same',
          renderLineHighlight: 'line',
          overviewRulerBorder: false,
          padding: { top: 12, bottom: 12 },
        }));
        this.layoutRawEditor();
      } catch (error) {
        this.rawEditorError = String(error);
      } finally {
        this.rawEditorLoading = false;
      }
    },
    updateRawEditor() {
      if (!this.rawEditor) return;
      const model = this.rawEditor.getModel();
      if (model && model.getValue() !== this.rawJson) {
        const position = this.rawEditor.getPosition();
        const scrollTop = this.rawEditor.getScrollTop();
        model.setValue(this.rawJson);
        if (position) this.rawEditor.setPosition(position);
        this.rawEditor.setScrollTop(scrollTop);
      }
      this.layoutRawEditor();
    },
    layoutRawEditor() {
      if (!this.rawEditor) return;
      window.requestAnimationFrame(() => {
        this.rawEditor?.layout();
      });
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
    tableFilteredRows(key, rows, fields) {
      const query = this.tableState[key]?.query?.trim().toLowerCase() ?? '';
      if (!query) return rows;
      return rows.filter((row) =>
        fields.some((field) => searchableText(fieldValue(row, field)).toLowerCase().includes(query)),
      );
    },
    tablePageRows(key, rows, fields) {
      const state = this.tableState[key];
      const filtered = this.tableFilteredRows(key, rows, fields);
      const totalPages = Math.max(1, Math.ceil(filtered.length / state.pageSize));
      const page = Math.min(Math.max(state.page, 1), totalPages);
      const start = (page - 1) * state.pageSize;
      return filtered.slice(start, start + state.pageSize);
    },
    tableMeta(key, rows, fields) {
      const state = this.tableState[key];
      const filtered = this.tableFilteredRows(key, rows, fields);
      const totalPages = Math.max(1, Math.ceil(filtered.length / state.pageSize));
      const page = Math.min(Math.max(state.page, 1), totalPages);
      const start = filtered.length ? (page - 1) * state.pageSize + 1 : 0;
      const end = Math.min(filtered.length, page * state.pageSize);
      return {
        page,
        totalPages,
        total: rows.length,
        filtered: filtered.length,
        start,
        end,
      };
    },
    resetTablePage(key) {
      this.tableState[key].page = 1;
    },
    setTablePage(key, page, rows, fields) {
      const meta = this.tableMeta(key, rows, fields);
      this.tableState[key].page = Math.min(Math.max(page, 1), meta.totalPages);
    },
    previousTablePage(key, rows, fields) {
      const meta = this.tableMeta(key, rows, fields);
      this.setTablePage(key, meta.page - 1, rows, fields);
    },
    nextTablePage(key, rows, fields) {
      const meta = this.tableMeta(key, rows, fields);
      this.setTablePage(key, meta.page + 1, rows, fields);
    },
    flattenUiElement(element, rows) {
      rows.push(element);
      for (const child of element.children ?? []) {
        this.flattenUiElement(child, rows);
      }
    },
    uiElementName(element) {
      return element.name || element.automation_id || element.class_name || element.element_ref || 'Unnamed element';
    },
    uiElementState(element) {
      const states = [];
      if (element.enabled === true) states.push('enabled');
      if (element.focused === true) states.push('focused');
      if (element.offscreen === true) states.push('offscreen');
      if (element.diagnostics?.length) states.push(`${element.diagnostics.length} diagnostics`);
      return states;
    },
    boundsText(bounds) {
      if (!bounds) return 'n/a';
      return `${bounds.x}, ${bounds.y} / ${bounds.width} x ${bounds.height}`;
    },
    uiaIndent(element) {
      return {
        paddingLeft: `${Math.min(element.depth || 0, 8) * 14}px`,
      };
    },
    detailPairs(value) {
      if (!value || typeof value !== 'object') return [];
      return Object.entries(value)
        .filter(([, detail]) => detail != null && detail !== '')
        .map(([key, detail]) => [key.replaceAll('_', ' '), compactValue(detail)]);
    },
    memoryDetailRows(item) {
      return [
        ['ID', item.id],
        ['Kind', item.kind],
        ['Created', this.formatStamp(item.created_at)],
        ['Updated', this.formatStamp(item.updated_at)],
        ['Use count', item.use_count],
        ['Last used', this.formatStamp(item.last_used_at)],
        ['Tags', item.tags?.join(', ')],
      ].filter(([, value]) => value != null && value !== '' && value !== 'n/a');
    },
    macroManifest(macro) {
      return macro.raw?.manifest ?? macro.raw?.manifest_json ?? macro.manifest ?? null;
    },
    macroDetailRows(macro) {
      const manifest = this.macroManifest(macro);
      return [
        ['Source', macro.source],
        ['Kind', macro.kind],
        ['Steps', macro.steps],
        ['Manifest version', manifest?.version],
        ['Launch tool', manifest?.launch?.tool],
        ['Bind strategy', manifest?.bind?.strategy],
        ['Required executable', manifest?.bind?.required_executable],
        ['App executable', manifest?.app_identity?.executable_name],
      ].filter(([, value]) => value != null && value !== '');
    },
    async copyMacroRunJson(item) {
      await this.copyJson({ manifest: this.macroManifest(item) ?? item.raw ?? item });
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
    async loadDocs() {
      if (this.docsLoaded || this.docsLoading) return;
      this.docsLoading = true;
      this.docsError = null;
      try {
        const response = await fetch('/dashboard/docs', {
          cache: 'no-store',
          headers: authHeaders(),
        });
        if (!response.ok) throw new Error(`HTTP ${response.status}`);
        const payload = await response.json();
        this.docs = payload.docs ?? [];
        this.docsLoaded = true;
        if (this.docs.length) {
          this.selectDoc(this.activeDocSlug ?? this.docs[0].slug);
        }
      } catch (error) {
        this.docsError = String(error);
      } finally {
        this.docsLoading = false;
      }
    },
    selectDoc(slug) {
      this.activeDocSlug = slug;
      this.renderMermaid();
    },
    docLinkFromHref(href) {
      if (!href || href.startsWith('#')) return null;
      let fileName = '';
      try {
        const url = new URL(href, window.location.href);
        if (url.origin !== window.location.origin) return null;
        fileName = url.pathname.split('/').pop() ?? '';
      } catch (error) {
        fileName = href.split('#')[0].split('?')[0].split('/').pop() ?? '';
      }
      if (!fileName.toLowerCase().endsWith('.md')) return null;
      const wanted = decodeURIComponent(fileName).replace(/\.md$/i, '').toLowerCase();
      return {
        fileName,
        slug: this.docs.find((doc) => doc.slug.toLowerCase() === wanted)?.slug ?? null,
      };
    },
    handleDocClick(event) {
      const link = event.target?.closest?.('a');
      if (!link) return;
      const docLink = this.docLinkFromHref(link.getAttribute('href'));
      if (!docLink) return;
      event.preventDefault();
      if (docLink.slug) {
        this.selectDoc(docLink.slug);
      } else {
        this.docsError = `Document not available: ${docLink.fileName}`;
      }
    },
    async renderMermaid() {
      await this.$nextTick();
      const host = this.$el?.querySelector?.('.winctl-markdown');
      if (!host) return;
      const blocks = host.querySelectorAll('code.language-mermaid');
      if (!blocks.length) return;
      const mermaid = await getMermaid();
      for (const code of blocks) {
        const target = code.closest('pre') ?? code;
        const definition = code.textContent ?? '';
        try {
          const { svg } = await mermaid.render(`winctl-mermaid-${mermaidSeq++}`, definition);
          const wrapper = document.createElement('div');
          wrapper.className = 'winctl-mermaid';
          wrapper.innerHTML = svg;
          target.replaceWith(wrapper);
        } catch (error) {
          target.classList.add('winctl-mermaid-render-failed');
        }
      }
    },
    toggleExpand(key) {
      this.expandedItems = { ...this.expandedItems, [key]: !this.expandedItems[key] };
    },
    formatStamp(value) {
      if (!value) return 'n/a';
      const date = new Date(value);
      return Number.isNaN(date.getTime()) ? String(value) : date.toLocaleString();
    },
    async deleteItem(id) {
      if (!id) return;
      if (!window.confirm('Delete this item permanently? This cannot be undone.')) return;
      this.deletingId = id;
      try {
        const response = await fetch('/dashboard/memory/delete', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json', ...authHeaders() },
          body: JSON.stringify({ id }),
        });
        const result = await response.json().catch(() => ({}));
        if (!response.ok || result.ok === false) {
          throw new Error(result?.error?.message ?? `HTTP ${response.status}`);
        }
        await this.loadState({ quiet: true });
      } catch (error) {
        window.alert(`Delete failed: ${error}`);
      } finally {
        this.deletingId = null;
      }
    },
  },
  template: `
    <div class="winctl-shell">
      <header class="winctl-topbar">
        <div class="flex w-full flex-col gap-5 px-4 py-5 sm:px-6 lg:px-8">
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

      <main class="w-full px-4 py-5 sm:px-6 lg:px-8">
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
          <div class="grid gap-4 sm:grid-cols-2 lg:grid-cols-5">
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
            <div class="winctl-table-toolbar">
              <input
                v-model="tableState.control.query"
                type="search"
                placeholder="Search events"
                class="input input-sm input-bordered w-full sm:max-w-xs"
                @input="resetTablePage('control')"
              />
              <span class="text-xs text-slate-500">
                Showing {{ tableMeta('control', controlEventRows, controlTableFields).start }}-{{ tableMeta('control', controlEventRows, controlTableFields).end }}
                of {{ tableMeta('control', controlEventRows, controlTableFields).filtered }}
              </span>
            </div>
            <div class="winctl-table-frame">
              <table class="table winctl-table table-sm">
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
                  <tr v-if="!tableMeta('control', controlEventRows, controlTableFields).filtered">
                    <td colspan="6" class="py-8 text-center text-slate-500">{{ tableState.control.query ? 'No matching control events' : 'No control events' }}</td>
                  </tr>
                  <tr v-for="event in tablePageRows('control', controlEventRows, controlTableFields)" :key="event.id">
                    <td class="text-xs">{{ formatUnixMs(event.timestamp_unix_ms) }}</td>
                    <td class="winctl-code text-xs">{{ event.kind }}</td>
                    <td><span class="badge badge-ghost badge-sm">{{ event.status }}</span></td>
                    <td class="winctl-code text-xs">{{ event.tool_name || 'n/a' }}</td>
                    <td class="winctl-code text-xs">{{ event.bound_id || 'n/a' }}</td>
                    <td class="text-xs">{{ event.message }}</td>
                  </tr>
                </tbody>
              </table>
            </div>
            <div class="winctl-table-footer">
              <button type="button" class="btn btn-xs" :disabled="tableMeta('control', controlEventRows, controlTableFields).page <= 1" @click="previousTablePage('control', controlEventRows, controlTableFields)">Previous</button>
              <span class="text-xs text-slate-500">Page {{ tableMeta('control', controlEventRows, controlTableFields).page }} of {{ tableMeta('control', controlEventRows, controlTableFields).totalPages }}</span>
              <button type="button" class="btn btn-xs" :disabled="tableMeta('control', controlEventRows, controlTableFields).page >= tableMeta('control', controlEventRows, controlTableFields).totalPages" @click="nextTablePage('control', controlEventRows, controlTableFields)">Next</button>
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
                <dl class="winctl-detail-grid">
                  <div v-for="[label, value] in screenshotDetails" :key="label" class="winctl-detail-row">
                    <dt>{{ label }}</dt>
                    <dd>{{ value }}</dd>
                  </div>
                </dl>
              </div>
            </div>
          </section>

          <section class="winctl-card overflow-hidden">
            <div class="winctl-card-header flex items-center justify-between gap-3 px-4 py-3">
              <h2 class="text-sm font-semibold">UI Automation tree</h2>
              <span class="badge badge-info badge-outline">{{ uiaElements.length }}</span>
            </div>
            <div v-if="!uiaSnapshot" class="px-4 py-8 text-center text-sm text-slate-500">Capture a UIA snapshot to inspect elements</div>
            <template v-else>
              <div class="winctl-detail-strip">
                <span>{{ uiaSnapshot.snapshot?.owner_window?.title || 'Untitled window' }}</span>
                <span>{{ uiaSnapshot.snapshot?.flattened_count || 0 }} elements</span>
                <span v-if="uiaSnapshot.snapshot?.truncated" class="text-warning">truncated</span>
              </div>
              <div class="winctl-table-toolbar">
                <input
                  v-model="tableState.uia.query"
                  type="search"
                  placeholder="Search UIA elements"
                  class="input input-sm input-bordered w-full sm:max-w-xs"
                  @input="resetTablePage('uia')"
                />
                <span class="text-xs text-slate-500">
                  Showing {{ tableMeta('uia', uiaElements, uiaTableFields).start }}-{{ tableMeta('uia', uiaElements, uiaTableFields).end }}
                  of {{ tableMeta('uia', uiaElements, uiaTableFields).filtered }}
                </span>
              </div>
              <div class="winctl-table-frame">
                <table class="table winctl-table table-sm">
                  <thead>
                    <tr><th>Element</th><th>Role</th><th>Automation ID</th><th>Class</th><th>Bounds</th><th>State</th></tr>
                  </thead>
                  <tbody>
                    <tr v-if="!tableMeta('uia', uiaElements, uiaTableFields).filtered">
                      <td colspan="6" class="py-8 text-center text-slate-500">No matching UIA elements</td>
                    </tr>
                    <tr v-for="element in tablePageRows('uia', uiaElements, uiaTableFields)" :key="element.element_ref">
                      <td>
                        <div class="font-medium" :style="uiaIndent(element)">{{ uiElementName(element) }}</div>
                        <div class="winctl-code text-[11px] text-slate-500">{{ element.element_ref }}</div>
                      </td>
                      <td>{{ element.role || 'n/a' }}</td>
                      <td class="winctl-code text-xs">{{ element.automation_id || 'n/a' }}</td>
                      <td class="winctl-code text-xs">{{ element.class_name || 'n/a' }}</td>
                      <td class="winctl-code text-xs">{{ boundsText(element.bounds) }}</td>
                      <td>
                        <div class="flex flex-wrap gap-1">
                          <span v-for="state in uiElementState(element)" :key="state" class="badge badge-ghost badge-xs">{{ state }}</span>
                          <span v-if="!uiElementState(element).length" class="text-xs text-slate-500">n/a</span>
                        </div>
                      </td>
                    </tr>
                  </tbody>
                </table>
              </div>
              <div class="winctl-table-footer">
                <button type="button" class="btn btn-xs" :disabled="tableMeta('uia', uiaElements, uiaTableFields).page <= 1" @click="previousTablePage('uia', uiaElements, uiaTableFields)">Previous</button>
                <span class="text-xs text-slate-500">Page {{ tableMeta('uia', uiaElements, uiaTableFields).page }} of {{ tableMeta('uia', uiaElements, uiaTableFields).totalPages }}</span>
                <button type="button" class="btn btn-xs" :disabled="tableMeta('uia', uiaElements, uiaTableFields).page >= tableMeta('uia', uiaElements, uiaTableFields).totalPages" @click="nextTablePage('uia', uiaElements, uiaTableFields)">Next</button>
              </div>
            </template>
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
                <dl class="winctl-detail-grid">
                  <div v-for="[label, value] in selectedDiffDetails" :key="label" class="winctl-detail-row">
                    <dt>{{ label }}</dt>
                    <dd>{{ value }}</dd>
                  </div>
                </dl>
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
                <dl class="winctl-detail-grid mt-3">
                  <div class="winctl-detail-row">
                    <dt>Run</dt>
                    <dd>{{ video.run_id }}</dd>
                  </div>
                  <div class="winctl-detail-row">
                    <dt>Path</dt>
                    <dd>{{ video.path }}</dd>
                  </div>
                  <div v-for="[label, value] in detailPairs(video.metadata)" :key="label" class="winctl-detail-row">
                    <dt>{{ label }}</dt>
                    <dd>{{ value }}</dd>
                  </div>
                </dl>
              </figure>
            </div>
          </section>

          <section class="winctl-card overflow-hidden">
            <div class="winctl-card-header flex items-center justify-between gap-3 px-4 py-3">
              <h2 class="text-sm font-semibold">Completions</h2>
              <span class="badge badge-info badge-outline">{{ macroResults.length }}</span>
            </div>
            <div class="winctl-table-toolbar">
              <input
                v-model="tableState.completions.query"
                type="search"
                placeholder="Search completions"
                class="input input-sm input-bordered w-full sm:max-w-xs"
                @input="resetTablePage('completions')"
              />
              <span class="text-xs text-slate-500">
                Showing {{ tableMeta('completions', macroResults, completionTableFields).start }}-{{ tableMeta('completions', macroResults, completionTableFields).end }}
                of {{ tableMeta('completions', macroResults, completionTableFields).filtered }}
              </span>
            </div>
            <div class="winctl-table-frame">
              <table class="table winctl-table table-sm">
                <thead>
                  <tr><th>Run</th><th>Status</th><th>Finished</th><th>Artifacts</th></tr>
                </thead>
                <tbody>
                  <tr v-if="!tableMeta('completions', macroResults, completionTableFields).filtered">
                    <td colspan="4" class="py-8 text-center text-slate-500">{{ tableState.completions.query ? 'No matching completions' : 'No completed macro or test runs' }}</td>
                  </tr>
                  <tr v-for="run in tablePageRows('completions', macroResults, completionTableFields)" :key="run.run_id">
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
            <div class="winctl-table-footer">
              <button type="button" class="btn btn-xs" :disabled="tableMeta('completions', macroResults, completionTableFields).page <= 1" @click="previousTablePage('completions', macroResults, completionTableFields)">Previous</button>
              <span class="text-xs text-slate-500">Page {{ tableMeta('completions', macroResults, completionTableFields).page }} of {{ tableMeta('completions', macroResults, completionTableFields).totalPages }}</span>
              <button type="button" class="btn btn-xs" :disabled="tableMeta('completions', macroResults, completionTableFields).page >= tableMeta('completions', macroResults, completionTableFields).totalPages" @click="nextTablePage('completions', macroResults, completionTableFields)">Next</button>
            </div>
          </section>
        </section>

        <section v-if="selectedTab === 'catalog'" class="winctl-card overflow-hidden">
          <div class="winctl-card-header flex items-center justify-between gap-3 px-4 py-3">
            <h2 class="text-sm font-semibold">Manifest catalog</h2>
            <span class="badge badge-info badge-outline">{{ macroItems.length }}</span>
          </div>
          <div class="winctl-table-toolbar">
            <input
              v-model="tableState.catalog.query"
              type="search"
              placeholder="Search manifests"
              class="input input-sm input-bordered w-full sm:max-w-xs"
              @input="resetTablePage('catalog')"
            />
            <span class="text-xs text-slate-500">
              Showing {{ tableMeta('catalog', macroItems, catalogCollectionFields).start }}-{{ tableMeta('catalog', macroItems, catalogCollectionFields).end }}
              of {{ tableMeta('catalog', macroItems, catalogCollectionFields).filtered }}
            </span>
          </div>
          <div class="grid gap-3 p-4 lg:grid-cols-2">
            <article v-if="!tableMeta('catalog', macroItems, catalogCollectionFields).filtered" class="py-8 text-center text-sm text-slate-500 lg:col-span-2">
              {{ tableState.catalog.query ? 'No matching saved macros or test manifests' : 'No saved macros or test manifests' }}
            </article>
            <article v-for="item in tablePageRows('catalog', macroItems, catalogCollectionFields)" :key="item.key" class="winctl-catalog-item">
              <div class="flex items-start justify-between gap-3">
                <div>
                  <h3 class="text-sm font-semibold">{{ manifestTitle(item) }}</h3>
                  <p class="mt-1 text-xs text-slate-500">{{ manifestDescription(item) }}</p>
                </div>
                <button type="button" class="btn btn-xs" @click="copyMacroRunJson(item)">Copy run JSON</button>
              </div>
              <div class="mt-3 flex flex-wrap gap-1">
                <span class="badge badge-ghost badge-sm">{{ item.kind || item.manifest?.version || 'manifest' }}</span>
                <span v-for="tag in item.tags ?? item.manifest?.tags ?? []" :key="tag" class="badge badge-outline badge-sm">{{ tag }}</span>
              </div>
            </article>
          </div>
          <div class="winctl-table-footer">
            <button type="button" class="btn btn-xs" :disabled="tableMeta('catalog', macroItems, catalogCollectionFields).page <= 1" @click="previousTablePage('catalog', macroItems, catalogCollectionFields)">Previous</button>
            <span class="text-xs text-slate-500">Page {{ tableMeta('catalog', macroItems, catalogCollectionFields).page }} of {{ tableMeta('catalog', macroItems, catalogCollectionFields).totalPages }}</span>
            <button type="button" class="btn btn-xs" :disabled="tableMeta('catalog', macroItems, catalogCollectionFields).page >= tableMeta('catalog', macroItems, catalogCollectionFields).totalPages" @click="nextTablePage('catalog', macroItems, catalogCollectionFields)">Next</button>
          </div>
        </section>

        <section v-if="selectedTab === 'windows'" class="winctl-card overflow-hidden">
          <div class="winctl-card-header flex items-center justify-between gap-3 px-4 py-3">
            <h2 class="text-sm font-semibold">Bound windows</h2>
            <span class="badge badge-info badge-outline">{{ boundWindows.length }}</span>
          </div>
          <div class="winctl-table-toolbar">
            <input
              v-model="tableState.windows.query"
              type="search"
              placeholder="Search windows"
              class="input input-sm input-bordered w-full sm:max-w-xs"
              @input="resetTablePage('windows')"
            />
            <span class="text-xs text-slate-500">
              Showing {{ tableMeta('windows', boundWindows, windowsTableFields).start }}-{{ tableMeta('windows', boundWindows, windowsTableFields).end }}
              of {{ tableMeta('windows', boundWindows, windowsTableFields).filtered }}
            </span>
          </div>
          <div class="winctl-table-frame">
            <table class="table winctl-table table-sm">
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
                <tr v-if="!tableMeta('windows', boundWindows, windowsTableFields).filtered">
                  <td colspan="6" class="py-8 text-center text-slate-500">{{ tableState.windows.query ? 'No matching bound windows' : 'No bound windows' }}</td>
                </tr>
                <tr v-for="bound in tablePageRows('windows', boundWindows, windowsTableFields)" :key="bound.bound_id">
                  <td>
                    <div class="font-medium">{{ boundTitle(bound) }}</div>
                    <div class="winctl-code text-xs text-slate-500">{{ bound.bound_id }}</div>
                  </td>
                  <td class="winctl-code text-xs">{{ bound.identity?.hwnd_hex }}</td>
                  <td class="winctl-code text-xs">{{ bound.identity?.pid }}</td>
                  <td>
                    <div>{{ bound.window?.process_name || 'n/a' }}</div>
                    <div class="text-xs text-slate-500">{{ shortPath(bound.window?.exe_path) }}</div>
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
          <div class="winctl-table-footer">
            <button type="button" class="btn btn-xs" :disabled="tableMeta('windows', boundWindows, windowsTableFields).page <= 1" @click="previousTablePage('windows', boundWindows, windowsTableFields)">Previous</button>
            <span class="text-xs text-slate-500">Page {{ tableMeta('windows', boundWindows, windowsTableFields).page }} of {{ tableMeta('windows', boundWindows, windowsTableFields).totalPages }}</span>
            <button type="button" class="btn btn-xs" :disabled="tableMeta('windows', boundWindows, windowsTableFields).page >= tableMeta('windows', boundWindows, windowsTableFields).totalPages" @click="nextTablePage('windows', boundWindows, windowsTableFields)">Next</button>
          </div>
        </section>

        <section v-if="selectedTab === 'processes'" class="winctl-card overflow-hidden">
          <div class="winctl-card-header flex items-center justify-between gap-3 px-4 py-3">
            <h2 class="text-sm font-semibold">Launched processes</h2>
            <span class="badge badge-info badge-outline">{{ launchedProcesses.length }}</span>
          </div>
          <div class="winctl-table-toolbar">
            <input
              v-model="tableState.processes.query"
              type="search"
              placeholder="Search processes"
              class="input input-sm input-bordered w-full sm:max-w-xs"
              @input="resetTablePage('processes')"
            />
            <span class="text-xs text-slate-500">
              Showing {{ tableMeta('processes', launchedProcesses, processTableFields).start }}-{{ tableMeta('processes', launchedProcesses, processTableFields).end }}
              of {{ tableMeta('processes', launchedProcesses, processTableFields).filtered }}
            </span>
          </div>
          <div class="winctl-table-frame">
            <table class="table winctl-table table-sm">
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
                <tr v-if="!tableMeta('processes', launchedProcesses, processTableFields).filtered">
                  <td colspan="5" class="py-8 text-center text-slate-500">{{ tableState.processes.query ? 'No matching launched processes' : 'No launched processes' }}</td>
                </tr>
                <tr v-for="process in tablePageRows('processes', launchedProcesses, processTableFields)" :key="process.launch_id">
                  <td>
                    <div class="font-medium">{{ processLabel(process) }}</div>
                    <div class="text-xs text-slate-500">{{ shortPath(process.executable_path) }}</div>
                  </td>
                  <td class="winctl-code text-xs">{{ process.pid }}</td>
                  <td class="winctl-code text-xs">{{ process.launch_id }}</td>
                  <td class="text-xs">{{ formatUnixMs(process.launch_time_unix_ms) }}</td>
                  <td class="winctl-code text-xs">{{ process.command_line }}</td>
                </tr>
              </tbody>
            </table>
          </div>
          <div class="winctl-table-footer">
            <button type="button" class="btn btn-xs" :disabled="tableMeta('processes', launchedProcesses, processTableFields).page <= 1" @click="previousTablePage('processes', launchedProcesses, processTableFields)">Previous</button>
            <span class="text-xs text-slate-500">Page {{ tableMeta('processes', launchedProcesses, processTableFields).page }} of {{ tableMeta('processes', launchedProcesses, processTableFields).totalPages }}</span>
            <button type="button" class="btn btn-xs" :disabled="tableMeta('processes', launchedProcesses, processTableFields).page >= tableMeta('processes', launchedProcesses, processTableFields).totalPages" @click="nextTablePage('processes', launchedProcesses, processTableFields)">Next</button>
          </div>
        </section>

        <section v-if="selectedTab === 'memory'" class="grid gap-5 lg:grid-cols-2">
          <section class="winctl-card overflow-hidden">
            <div class="winctl-card-header flex items-center justify-between gap-3 px-4 py-3">
              <h2 class="text-sm font-semibold">Memory</h2>
              <span class="badge badge-info badge-outline">{{ memoryItems.length }}</span>
            </div>
            <div class="winctl-table-toolbar">
              <input
                v-model="tableState.memory.query"
                type="search"
                placeholder="Search memory"
                class="input input-sm input-bordered w-full sm:max-w-xs"
                @input="resetTablePage('memory')"
              />
              <span class="text-xs text-slate-500">
                Showing {{ tableMeta('memory', memoryItems, memoryCollectionFields).start }}-{{ tableMeta('memory', memoryItems, memoryCollectionFields).end }}
                of {{ tableMeta('memory', memoryItems, memoryCollectionFields).filtered }}
              </span>
            </div>
            <div class="winctl-collection-list space-y-3 p-3">
              <p v-if="!tableMeta('memory', memoryItems, memoryCollectionFields).filtered" class="px-1 py-6 text-center text-sm opacity-60">
                {{ tableState.memory.query ? 'No matching memory items.' : 'No memory items.' }}
              </p>
              <article
                v-for="item in tablePageRows('memory', memoryItems, memoryCollectionFields)"
                :key="item.id"
                class="rounded-lg border p-3"
                style="border-color: var(--winctl-border)"
              >
                <div class="flex items-start justify-between gap-2">
                  <div class="min-w-0">
                    <div class="flex flex-wrap items-center gap-2">
                      <span class="badge badge-sm badge-ghost">{{ item.kind }}</span>
                      <h3 class="truncate text-sm font-semibold">{{ item.title || item.id }}</h3>
                    </div>
                    <p class="mt-1 whitespace-pre-wrap break-words text-xs opacity-80">{{ item.text }}</p>
                  </div>
                  <button
                    class="btn btn-ghost btn-xs text-error shrink-0"
                    :disabled="deletingId === item.id"
                    title="Delete"
                    @click="deleteItem(item.id)"
                  >Delete</button>
                </div>
                <div v-if="item.tags?.length" class="mt-2 flex flex-wrap gap-1">
                  <span v-for="tag in item.tags" :key="tag" class="badge badge-outline badge-xs">{{ tag }}</span>
                </div>
                <div class="mt-2 flex items-center justify-between text-[11px] opacity-60">
                  <span>updated {{ formatStamp(item.updated_at) }}</span>
                  <button class="link link-hover" @click="toggleExpand('mem:' + item.id)">
                    {{ expandedItems['mem:' + item.id] ? 'Hide details' : 'Details' }}
                  </button>
                </div>
                <dl v-if="expandedItems['mem:' + item.id]" class="winctl-detail-grid mt-2">
                  <div v-for="[label, value] in memoryDetailRows(item)" :key="label" class="winctl-detail-row">
                    <dt>{{ label }}</dt>
                    <dd>{{ value }}</dd>
                  </div>
                </dl>
              </article>
            </div>
            <div class="winctl-table-footer">
              <button type="button" class="btn btn-xs" :disabled="tableMeta('memory', memoryItems, memoryCollectionFields).page <= 1" @click="previousTablePage('memory', memoryItems, memoryCollectionFields)">Previous</button>
              <span class="text-xs text-slate-500">Page {{ tableMeta('memory', memoryItems, memoryCollectionFields).page }} of {{ tableMeta('memory', memoryItems, memoryCollectionFields).totalPages }}</span>
              <button type="button" class="btn btn-xs" :disabled="tableMeta('memory', memoryItems, memoryCollectionFields).page >= tableMeta('memory', memoryItems, memoryCollectionFields).totalPages" @click="nextTablePage('memory', memoryItems, memoryCollectionFields)">Next</button>
            </div>
          </section>
          <section class="winctl-card overflow-hidden">
            <div class="winctl-card-header flex items-center justify-between gap-3 px-4 py-3">
              <h2 class="text-sm font-semibold">Macros</h2>
              <span class="badge badge-info badge-outline">{{ macroItems.length }}</span>
            </div>
            <div class="winctl-table-toolbar">
              <input
                v-model="tableState.macros.query"
                type="search"
                placeholder="Search macros"
                class="input input-sm input-bordered w-full sm:max-w-xs"
                @input="resetTablePage('macros')"
              />
              <span class="text-xs text-slate-500">
                Showing {{ tableMeta('macros', macroItems, macroCollectionFields).start }}-{{ tableMeta('macros', macroItems, macroCollectionFields).end }}
                of {{ tableMeta('macros', macroItems, macroCollectionFields).filtered }}
              </span>
            </div>
            <div class="winctl-collection-list space-y-3 p-3">
              <p v-if="!tableMeta('macros', macroItems, macroCollectionFields).filtered" class="px-1 py-6 text-center text-sm opacity-60">
                {{ tableState.macros.query ? 'No matching macros.' : 'No macros.' }}
              </p>
              <article
                v-for="macro in tablePageRows('macros', macroItems, macroCollectionFields)"
                :key="macro.key"
                class="rounded-lg border p-3"
                style="border-color: var(--winctl-border)"
              >
                <div class="flex items-start justify-between gap-2">
                  <div class="min-w-0">
                    <div class="flex flex-wrap items-center gap-2">
                      <span class="badge badge-sm badge-ghost">{{ macro.kind }}</span>
                      <span class="badge badge-sm" :class="macro.source === 'memory' ? 'badge-info' : 'badge-neutral'">{{ macro.source }}</span>
                      <h3 class="truncate text-sm font-semibold">{{ macro.title }}</h3>
                    </div>
                    <p v-if="macro.description" class="mt-1 whitespace-pre-wrap break-words text-xs opacity-80">{{ macro.description }}</p>
                  </div>
                  <button
                    v-if="macro.deleteId"
                    class="btn btn-ghost btn-xs text-error shrink-0"
                    :disabled="deletingId === macro.deleteId"
                    title="Delete"
                    @click="deleteItem(macro.deleteId)"
                  >Delete</button>
                  <span v-else class="shrink-0 text-[11px] opacity-50" title="Session-only macros are not stored and cannot be deleted here">session-only</span>
                </div>
                <div v-if="macro.tags?.length" class="mt-2 flex flex-wrap gap-1">
                  <span v-for="tag in macro.tags" :key="tag" class="badge badge-outline badge-xs">{{ tag }}</span>
                </div>
                <div class="mt-2 flex items-center justify-between text-[11px] opacity-60">
                  <span v-if="macro.steps != null">{{ macro.steps }} steps</span>
                  <span v-else></span>
                  <button class="link link-hover" @click="toggleExpand(macro.key)">
                    {{ expandedItems[macro.key] ? 'Hide details' : 'Details' }}
                  </button>
                </div>
                <div v-if="expandedItems[macro.key]" class="mt-2 space-y-2">
                  <dl class="winctl-detail-grid">
                    <div v-for="[label, value] in macroDetailRows(macro)" :key="label" class="winctl-detail-row">
                      <dt>{{ label }}</dt>
                      <dd>{{ value }}</dd>
                    </div>
                  </dl>
                  <button type="button" class="btn btn-xs" @click="copyMacroRunJson(macro)">Copy run JSON</button>
                </div>
              </article>
            </div>
            <div class="winctl-table-footer">
              <button type="button" class="btn btn-xs" :disabled="tableMeta('macros', macroItems, macroCollectionFields).page <= 1" @click="previousTablePage('macros', macroItems, macroCollectionFields)">Previous</button>
              <span class="text-xs text-slate-500">Page {{ tableMeta('macros', macroItems, macroCollectionFields).page }} of {{ tableMeta('macros', macroItems, macroCollectionFields).totalPages }}</span>
              <button type="button" class="btn btn-xs" :disabled="tableMeta('macros', macroItems, macroCollectionFields).page >= tableMeta('macros', macroItems, macroCollectionFields).totalPages" @click="nextTablePage('macros', macroItems, macroCollectionFields)">Next</button>
            </div>
          </section>
        </section>

        <section v-if="selectedTab === 'docs'" class="grid gap-5 lg:grid-cols-[260px_1fr]">
          <section class="winctl-card overflow-hidden">
            <div class="winctl-card-header px-4 py-3">
              <h2 class="text-sm font-semibold">Tool docs</h2>
            </div>
            <div class="p-3 space-y-3">
              <input
                v-model="docsSearch"
                type="search"
                placeholder="Search docs"
                class="input input-sm input-bordered w-full"
              />
              <div v-if="docsLoading" class="px-1 text-sm opacity-70">Loading docs…</div>
              <div v-else-if="docsError" class="px-1 text-sm text-error">{{ docsError }}</div>
              <ul v-else class="menu menu-sm w-full p-0 max-h-[70vh] flex-nowrap overflow-y-auto">
                <li v-for="doc in filteredDocs" :key="doc.slug">
                  <a
                    :class="{ active: doc.slug === activeDocSlug }"
                    @click="selectDoc(doc.slug)"
                  >{{ doc.title }}</a>
                </li>
                <li v-if="!filteredDocs.length" class="px-2 py-1 text-sm opacity-60">No matches</li>
              </ul>
            </div>
          </section>
          <section class="winctl-card overflow-hidden">
            <div class="winctl-card-header px-4 py-3">
              <h2 class="text-sm font-semibold">{{ activeDoc?.title ?? 'Select a document' }}</h2>
            </div>
            <div
              class="winctl-markdown max-h-[80vh] overflow-y-auto p-5"
              @click="handleDocClick"
              v-html="activeDoc?.html ?? '<p class=&quot;opacity-60&quot;>Choose a tool doc from the list.</p>'"
            ></div>
          </section>
        </section>

        <section v-show="selectedTab === 'raw'" class="winctl-card winctl-raw-card overflow-hidden" :class="{ 'winctl-raw-card-maximized': rawEditorMaximized }">
          <div class="winctl-card-header flex items-center justify-between gap-3 px-4 py-3">
            <h2 class="text-sm font-semibold">Dashboard state</h2>
            <button type="button" class="btn btn-xs" @click="rawEditorMaximized = !rawEditorMaximized">
              {{ rawEditorMaximized ? 'Restore' : 'Maximize' }}
            </button>
          </div>
          <div v-if="rawEditorError" class="alert alert-error m-4 text-sm">{{ rawEditorError }}</div>
          <div class="winctl-monaco-shell">
            <div v-if="rawEditorLoading" class="winctl-monaco-loading text-sm text-slate-500">Loading editor</div>
            <div ref="rawEditor" class="winctl-monaco-host" aria-label="Dashboard state JSON"></div>
          </div>
        </section>
      </main>
    </div>
  `,
}).mount('#app');
