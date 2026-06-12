import { createApp } from 'vue/dist/vue.esm-bundler.js';
import './styles.css';
import icon32Url from '../../../../assets/brand/winctl-icon-32.png';
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

const params = new URLSearchParams(window.location.search);
const urlToken = params.get('token');
if (urlToken) {
  sessionStorage.setItem('winctl.dashboard.token', urlToken);
}
const authToken = urlToken || sessionStorage.getItem('winctl.dashboard.token');

const DASHBOARD_TABS = [
  { id: 'overview', label: 'Overview' },
  { id: 'control', label: 'Control' },
  { id: 'observability', label: 'Observability' },
  { id: 'connect', label: 'Connect' },
  { id: 'recorder', label: 'Recorder' },
  { id: 'inspect', label: 'Inspect' },
  { id: 'artifacts', label: 'Artifacts' },
  { id: 'catalog', label: 'Catalog' },
  { id: 'windows', label: 'Windows' },
  { id: 'processes', label: 'Processes' },
  { id: 'memory', label: 'Memory' },
  { id: 'docs', label: 'Docs' },
  { id: 'config', label: 'Settings' },
];
const requestedTab = params.get('tab');
const initialTab = DASHBOARD_TABS.some((tab) => tab.id === requestedTab) ? requestedTab : 'overview';

function setFavicon(href) {
  const existing = document.querySelector('link[rel="icon"]');
  const link = existing || document.createElement('link');
  link.rel = 'icon';
  link.type = 'image/png';
  link.href = href;
  if (!existing) document.head.appendChild(link);
}

setFavicon(icon32Url);

function authHeaders() {
  return authToken ? { Authorization: `Bearer ${authToken}` } : {};
}

function jsonSnippet(value) {
  return JSON.stringify(value, null, 2);
}

function psSingleQuote(value) {
  return `'${String(value).replaceAll("'", "''")}'`;
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

const TOOL_DOC_PREFIXES = [
  ['APP', 'app'],
  ['ARTIFACT', 'artifact'],
  ['ASSERT', 'assert'],
  ['BROWSER', 'browser'],
  ['BUILD', 'build'],
  ['CAPTURE', 'capture'],
  ['CLIPBOARD', 'clipboard'],
  ['CONTROL', 'control'],
  ['DIAGNOSTICS', 'diagnostics'],
  ['DIALOGS', 'dialogs'],
  ['FILESYSTEM', 'filesystem'],
  ['INPUT', 'input'],
  ['MACRO', 'macro'],
  ['MEMORY', 'memory'],
  ['NETWORK', 'network'],
  ['NOTIFICATIONS', 'notifications'],
  ['PROCESS', 'process'],
  ['RECORDER', 'recorder'],
  ['REGISTRY', 'registry'],
  ['SERVER', 'server'],
  ['TEST', 'test'],
  ['UIA', 'uia'],
  ['WEB', 'web'],
  ['WINDOWS', 'windows'],
];
const TOOL_DOC_PREFIX_LABELS = new Map(TOOL_DOC_PREFIXES);
const PROJECT_DOC_SLUGS = new Set([
  'INDEX',
  'CLIENT_CONFIGS',
  'CONFIGURATION',
  'DASHBOARD',
  'DIRECTORY_LAYOUT',
  'DISTRIBUTION',
  'MACRO_MANIFEST',
  'MINILM_EMBEDDINGS',
  'RECORDER',
  'SERVER_OBSERVABILITY',
  'TEST_MANIFEST',
  'TRAY_CONTROLLER',
  'TROUBLESHOOTING',
  'UIA_ACTIONS_DESIGN',
  'WINDOWS-RUNBOOK',
]);

function docSearchText(doc) {
  return `${doc.title ?? ''} ${doc.slug ?? ''}`.toLowerCase();
}

function toolDocPrefix(doc) {
  const slug = String(doc.slug ?? '').toUpperCase();
  if (PROJECT_DOC_SLUGS.has(slug)) return null;
  const separator = slug.indexOf('_');
  if (separator <= 0) return null;
  const prefix = slug.slice(0, separator);
  return TOOL_DOC_PREFIX_LABELS.has(prefix) ? prefix : null;
}

function docGroupFor(doc) {
  const prefix = toolDocPrefix(doc);
  if (!prefix) {
    return { id: 'project', label: 'Project docs', kind: 'project' };
  }
  const label = TOOL_DOC_PREFIX_LABELS.get(prefix);
  return { id: `tool:${prefix}`, label: `${label}.*`, kind: 'tool', prefix };
}

createApp({
  data() {
    return {
      logoUrl,
      data: null,
      loading: true,
      refreshing: false,
      error: null,
      selectedTab: initialTab,
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
      collapsedDocGroups: {},
      expandedItems: {},
      deletingId: null,
      connectState: null,
      connectLoading: false,
      connectError: null,
      connectTokenLabel: '',
      connectTokenBusy: false,
      connectTokenValue: '',
      connectTokenMetadata: null,
      connectDashboardTokenVisible: false,
      connectMode: 'http',
      copiedConnectId: '',
      settingsLoading: false,
      settingsError: null,
      settingsSaving: false,
      settingsRestarting: false,
      settingsEditable: false,
      settingsTrayAvailable: false,
      settingsConfigFile: null,
      settingsConnection: { transport: '', listen: '', auth_required: false, auth_token_set: false },
      settingsConfirmOpen: false,
      settingsNotice: '',
      settingsFieldErrors: [],
      settingsForm: {
        policy: {
          enable_filesystem_mutation: false,
          enable_clipboard_write: false,
          enable_registry_mutation: false,
          allow_private_network: false,
          memory_mutation_enabled: true,
          macro_execution_enabled: true,
          macro_destructive_tools_allowed: false,
          max_macro_runtime_ms: null,
          max_macro_steps: null,
          screenshot_retention_count: null,
          tool_allowlist: '',
          tool_denylist: '',
        },
        paths: { capture_dir: '', artifact_dir: '', filesystem_roots: '', memory_db: '' },
        logging: { log_file: '' },
        embedding: { model_path: '', dimension: null },
        macro_execution: { enabled: true, allow_destructive_tools: false, max_runtime_ms: null, max_steps: null },
      },
      issuePanelOpen: false,
      copiedIssueState: false,
      recorderTitle: 'Recorded macro',
      recorderDescription: '',
      recorderRemember: true,
      recorderBusy: false,
      recorderError: null,
      selectedRecordingId: 'active',
      tableState: {
        control: { query: '', page: 1, pageSize: 10 },
        catalog: { query: '', page: 1, pageSize: 12 },
        completions: { query: '', page: 1, pageSize: 10 },
        clients: { query: '', page: 1, pageSize: 8 },
        requests: { query: '', page: 1, pageSize: 12 },
        windows: { query: '', page: 1, pageSize: 10 },
        processes: { query: '', page: 1, pageSize: 10 },
        uia: { query: '', page: 1, pageSize: 12 },
        memory: { query: '', page: 1, pageSize: 8 },
        macros: { query: '', page: 1, pageSize: 8 },
      },
      tabs: DASHBOARD_TABS,
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
    connectedClients() {
      return this.data?.connected_clients ?? [];
    },
    recentRequests() {
      return (this.data?.recent_requests ?? []).slice().reverse();
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
    recordingState() {
      return this.data?.recording ?? {};
    },
    recordingActive() {
      return this.recordingState.active ?? null;
    },
    recordingCompleted() {
      return this.recordingState.completed ?? [];
    },
    recordingCompletedManifests() {
      return this.recordingState.completed_manifests ?? [];
    },
    recorderNativeCapture() {
      return this.recordingState.native_capture ?? {};
    },
    recorderSessionChoices() {
      const choices = [];
      if (this.recordingActive) {
        choices.push({
          id: 'active',
          label: `${this.recordingActive.title || 'Active recording'} (active)`,
          session: this.recordingActive,
          manifest: this.recordingState.active_manifest,
          validation: null,
        });
      }
      for (const session of this.recordingCompleted) {
        const manifestRecord = this.recordingCompletedManifests.find(
          (item) => item.session_id === session.id,
        );
        choices.push({
          id: session.id,
          label: session.title || session.id,
          session,
          manifest: manifestRecord?.manifest ?? null,
          validation: manifestRecord?.validation ?? null,
        });
      }
      return choices;
    },
    selectedRecording() {
      if (this.selectedRecordingId === 'active' && this.recordingActive) {
        return this.recorderSessionChoices.find((choice) => choice.id === 'active') ?? null;
      }
      return (
        this.recorderSessionChoices.find((choice) => choice.id === this.selectedRecordingId) ??
        this.recorderSessionChoices[0] ??
        null
      );
    },
    selectedRecordingManifest() {
      return this.selectedRecording?.manifest ?? null;
    },
    selectedRecordingSteps() {
      return this.selectedRecordingManifest?.steps ?? this.selectedRecording?.session?.steps ?? [];
    },
    recorderInference() {
      return this.selectedRecordingManifest?.replay?.extra?.recorder?.launch_inference ?? null;
    },
    recorderStatusBadgeClass() {
      const status = this.recordingState.status;
      return status === 'recording'
        ? 'badge-success'
        : status === 'paused'
          ? 'badge-warning'
          : 'badge-ghost';
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
    clientTableFields() {
      return [
        'connection_id',
        'name',
        'version',
        'transport',
        'last_tool',
        'request_count',
      ];
    },
    requestTableFields() {
      return [
        'connection_id',
        'tool_name',
        'ok',
        'error_code',
        'summary.tool',
        'summary.args',
        'summary.result',
      ];
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
    connectMcpUrl() {
      return this.connectState?.mcp_url ?? `${window.location.origin}/mcp`;
    },
    connectServerExe() {
      return this.connectState?.server_exe ?? 'winctl-mcp-server.exe';
    },
    connectStdioArgs() {
      return ['serve', '--transport', 'stdio'];
    },
    connectTokens() {
      return this.connectState?.tokens ?? [];
    },
    connectExampleToken() {
      return this.connectTokenValue || authToken || '<token>';
    },
    currentDashboardToken() {
      return authToken || '';
    },
    connectAuthHeader() {
      return `Bearer ${this.connectExampleToken}`;
    },
    connectExamples() {
      if (this.connectMode === 'stdio') return this.connectStdioExamples;
      const url = this.connectMcpUrl;
      const token = this.connectExampleToken;
      const authHeader = this.connectAuthHeader;
      const codexConfig = `[mcp_servers.winctl-mcp]
url = "${url}"
http_headers = { Authorization = "${authHeader}" }
tool_timeout_sec = 120.0`;
      const cursorConfig = jsonSnippet({
        mcpServers: {
          'winctl-mcp': {
            url,
            headers: { Authorization: authHeader },
          },
        },
      });
      const copilotConfig = jsonSnippet({
        servers: {
          'winctl-mcp': {
            type: 'http',
            url,
            headers: { Authorization: authHeader },
          },
        },
      });
      const copilotAdd = jsonSnippet({
        name: 'winctl-mcp',
        type: 'http',
        url,
        headers: { Authorization: authHeader },
      });
      return [
        {
          id: 'codex',
          title: 'Codex',
          path: '~/.codex/config.toml',
          command: `$env:WINCTL_MCP_TOKEN = ${psSingleQuote(token)}
codex mcp add winctl-mcp --url ${psSingleQuote(url)} --bearer-token-env-var WINCTL_MCP_TOKEN`,
          config: codexConfig,
        },
        {
          id: 'claude-code',
          title: 'Claude Code',
          path: 'project or user MCP config',
          command: `claude mcp add --transport http winctl-mcp ${psSingleQuote(url)} --header ${psSingleQuote(`Authorization: ${authHeader}`)}`,
          config: jsonSnippet({
            type: 'http',
            url,
            headers: { Authorization: authHeader },
          }),
        },
        {
          id: 'cursor',
          title: 'Cursor',
          path: '.cursor/mcp.json',
          command: `New-Item -ItemType Directory -Force .cursor
Set-Content -Path .cursor\\mcp.json -Value @'
${cursorConfig}
'@`,
          config: cursorConfig,
        },
        {
          id: 'github-copilot',
          title: 'GitHub Copilot',
          path: '.vscode/mcp.json',
          command: `code --add-mcp ${psSingleQuote(copilotAdd)}`,
          config: copilotConfig,
        },
      ];
    },
    connectStdioExamples() {
      const command = this.connectServerExe;
      const args = this.connectStdioArgs;
      const codexConfig = `[mcp_servers.winctl-mcp]
command = "${command.replaceAll('\\', '\\\\')}"
args = ${jsonSnippet(args)}
tool_timeout_sec = 120.0`;
      const claudeConfig = jsonSnippet({
        mcpServers: {
          'winctl-mcp': {
            command,
            args,
          },
        },
      });
      const cursorConfig = jsonSnippet({
        mcpServers: {
          'winctl-mcp': {
            command,
            args,
          },
        },
      });
      const copilotConfig = jsonSnippet({
        servers: {
          'winctl-mcp': {
            type: 'stdio',
            command,
            args,
          },
        },
      });
      const copilotAdd = jsonSnippet({
        name: 'winctl-mcp',
        command,
        args,
      });
      const cliArgs = args.map(psSingleQuote).join(' ');
      return [
        {
          id: 'codex-stdio',
          title: 'Codex',
          path: '~/.codex/config.toml',
          command: `codex mcp add winctl-mcp -- ${psSingleQuote(command)} ${cliArgs}`,
          config: codexConfig,
        },
        {
          id: 'claude-code-stdio',
          title: 'Claude Code',
          path: 'Claude MCP config',
          command: `claude mcp add winctl-mcp -- ${psSingleQuote(command)} ${cliArgs}`,
          config: claudeConfig,
        },
        {
          id: 'cursor-stdio',
          title: 'Cursor',
          path: '.cursor/mcp.json',
          command: `New-Item -ItemType Directory -Force .cursor
Set-Content -Path .cursor\\mcp.json -Value @'
${cursorConfig}
'@`,
          config: cursorConfig,
        },
        {
          id: 'github-copilot-stdio',
          title: 'GitHub Copilot',
          path: '.vscode/mcp.json',
          command: `code --add-mcp ${psSingleQuote(copilotAdd)}`,
          config: copilotConfig,
        },
      ];
    },
    issueUrl() {
      return 'https://github.com/phenixrizen/winctl-mcp/issues/new';
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
        connected_clients: this.connectedClients,
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
      return this.docs.filter((doc) => docSearchText(doc).includes(query));
    },
    docGroups() {
      const projectGroup = { id: 'project', label: 'Project docs', kind: 'project', docs: [] };
      const toolGroups = new Map();
      for (const doc of this.docs) {
        const groupInfo = docGroupFor(doc);
        if (groupInfo.kind === 'project') {
          projectGroup.docs.push(doc);
          continue;
        }
        if (!toolGroups.has(groupInfo.id)) {
          toolGroups.set(groupInfo.id, { ...groupInfo, docs: [] });
        }
        toolGroups.get(groupInfo.id).docs.push(doc);
      }
      const orderedToolGroups = TOOL_DOC_PREFIXES
        .map(([prefix]) => toolGroups.get(`tool:${prefix}`))
        .filter(Boolean);
      const groups = [];
      if (projectGroup.docs.length) groups.push(projectGroup);
      return groups.concat(orderedToolGroups);
    },
    filteredDocGroups() {
      const query = this.docsSearch.trim().toLowerCase();
      return this.docGroups
        .map((group) => ({
          ...group,
          docs: query ? group.docs.filter((doc) => docSearchText(doc).includes(query)) : group.docs,
        }))
        .filter((group) => group.docs.length);
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
      if (tab === 'connect') await this.loadConnect();
      if (tab === 'config') this.loadSettings();
      if (tab === 'docs') {
        await this.loadDocs();
        await this.renderMermaid();
      }
    },
  },
  mounted() {
    this.loadState();
    if (this.selectedTab === 'connect') this.loadConnect();
    if (this.selectedTab === 'config') this.loadSettings();
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
    async loadConnect() {
      this.connectLoading = true;
      this.connectError = null;
      try {
        const response = await fetch('/dashboard/connect', {
          cache: 'no-store',
          headers: authHeaders(),
        });
        if (!response.ok) throw new Error(`HTTP ${response.status}`);
        this.connectState = await response.json();
      } catch (error) {
        this.connectError = String(error);
      } finally {
        this.connectLoading = false;
      }
    },
    async connectPost(path, body) {
      this.connectTokenBusy = true;
      this.connectError = null;
      try {
        const response = await fetch(path, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json', ...authHeaders() },
          body: JSON.stringify(body ?? {}),
        });
        const payload = await response.json().catch(() => ({}));
        if (!response.ok || payload.ok === false) {
          throw new Error(payload.error?.message ?? `HTTP ${response.status}`);
        }
        if (payload.tokens) {
          this.connectState = { ...(this.connectState ?? {}), tokens: payload.tokens };
        }
        return payload;
      } catch (error) {
        this.connectError = String(error);
        return null;
      } finally {
        this.connectTokenBusy = false;
      }
    },
    async createConnectToken() {
      const payload = await this.connectPost('/dashboard/connect/token', {
        label: this.connectTokenLabel || null,
      });
      if (!payload) return;
      this.connectTokenValue = payload.token ?? '';
      this.connectTokenMetadata = payload.metadata ?? null;
      this.connectTokenLabel = '';
    },
    async revealConnectToken(token) {
      const payload = await this.connectPost('/dashboard/connect/token/reveal', { id: token.id });
      if (!payload) return;
      this.connectTokenValue = payload.token ?? '';
      this.connectTokenMetadata = payload.metadata ?? token;
    },
    async revokeConnectToken(token) {
      const confirmed = window.confirm(`Revoke ${token.label || token.id}? Active clients using it will stop connecting.`);
      if (!confirmed) return;
      const payload = await this.connectPost('/dashboard/connect/token/revoke', { id: token.id });
      if (!payload) return;
      if (this.connectTokenMetadata?.id === token.id) {
        this.connectTokenValue = '';
        this.connectTokenMetadata = null;
      }
    },
    async copyConnectText(id, text) {
      await navigator.clipboard?.writeText(text).catch(() => {});
      this.copiedConnectId = id;
      window.setTimeout(() => {
        if (this.copiedConnectId === id) this.copiedConnectId = '';
      }, 1600);
    },
    settingsListToText(value) {
      return Array.isArray(value) ? value.join('\n') : '';
    },
    settingsTextToList(value) {
      return String(value || '')
        .split('\n')
        .map((line) => line.trim())
        .filter((line) => line.length > 0);
    },
    settingsNumOrNull(value) {
      if (value === null || value === undefined || value === '') return null;
      const n = Number(value);
      return Number.isFinite(n) ? n : null;
    },
    applySettingsResponse(payload) {
      this.settingsEditable = payload.editable === true;
      this.settingsTrayAvailable = payload.tray_available === true;
      this.settingsConfigFile = payload.config_file ?? null;
      this.settingsConnection = payload.connection ?? this.settingsConnection;
      const s = payload.sections ?? {};
      const p = s.policy ?? {};
      const paths = s.paths ?? {};
      const logging = s.logging ?? {};
      const emb = s.embedding ?? {};
      const macro = s.macro_execution ?? {};
      this.settingsForm = {
        policy: {
          enable_filesystem_mutation: !!p.enable_filesystem_mutation,
          enable_clipboard_write: !!p.enable_clipboard_write,
          enable_registry_mutation: !!p.enable_registry_mutation,
          allow_private_network: !!p.allow_private_network,
          memory_mutation_enabled: p.memory_mutation_enabled !== false,
          macro_execution_enabled: p.macro_execution_enabled !== false,
          macro_destructive_tools_allowed: !!p.macro_destructive_tools_allowed,
          max_macro_runtime_ms: p.max_macro_runtime_ms ?? null,
          max_macro_steps: p.max_macro_steps ?? null,
          screenshot_retention_count: p.screenshot_retention_count ?? null,
          tool_allowlist: this.settingsListToText(p.tool_allowlist),
          tool_denylist: this.settingsListToText(p.tool_denylist),
        },
        paths: {
          capture_dir: paths.capture_dir ?? '',
          artifact_dir: paths.artifact_dir ?? '',
          filesystem_roots: this.settingsListToText(paths.filesystem_roots),
          memory_db: paths.memory_db ?? '',
        },
        logging: { log_file: logging.log_file ?? '' },
        embedding: { model_path: emb.model_path ?? '', dimension: emb.dimension ?? null },
        macro_execution: {
          enabled: macro.enabled !== false,
          allow_destructive_tools: !!macro.allow_destructive_tools,
          max_runtime_ms: macro.max_runtime_ms ?? null,
          max_steps: macro.max_steps ?? null,
        },
      };
    },
    async loadSettings() {
      this.settingsLoading = true;
      this.settingsError = null;
      try {
        const response = await fetch('/dashboard/config', { headers: { ...authHeaders() } });
        const payload = await response.json().catch(() => ({}));
        if (!response.ok || payload.ok === false) {
          throw new Error(payload.reason || `HTTP ${response.status}`);
        }
        this.applySettingsResponse(payload);
      } catch (error) {
        this.settingsError = String(error);
      } finally {
        this.settingsLoading = false;
      }
    },
    settingsPayload() {
      const f = this.settingsForm;
      const orNull = (v) => (v.trim() === '' ? null : v.trim());
      return {
        policy: {
          ...f.policy,
          macro_execution_enabled: f.macro_execution.enabled,
          macro_destructive_tools_allowed: f.macro_execution.allow_destructive_tools,
          max_macro_runtime_ms: this.settingsNumOrNull(f.policy.max_macro_runtime_ms),
          max_macro_steps: this.settingsNumOrNull(f.policy.max_macro_steps),
          screenshot_retention_count: this.settingsNumOrNull(f.policy.screenshot_retention_count),
          tool_allowlist: this.settingsTextToList(f.policy.tool_allowlist),
          tool_denylist: this.settingsTextToList(f.policy.tool_denylist),
        },
        paths: {
          capture_dir: orNull(f.paths.capture_dir),
          artifact_dir: orNull(f.paths.artifact_dir),
          filesystem_roots: this.settingsTextToList(f.paths.filesystem_roots),
          memory_db: orNull(f.paths.memory_db),
        },
        logging: { log_file: orNull(f.logging.log_file) },
        embedding: {
          model_path: orNull(f.embedding.model_path),
          dimension: this.settingsNumOrNull(f.embedding.dimension),
        },
        macro_execution: {
          enabled: f.macro_execution.enabled,
          allow_destructive_tools: f.macro_execution.allow_destructive_tools,
          max_runtime_ms: this.settingsNumOrNull(f.macro_execution.max_runtime_ms),
          max_steps: this.settingsNumOrNull(f.macro_execution.max_steps),
        },
      };
    },
    async saveSettings() {
      this.settingsSaving = true;
      this.settingsError = null;
      this.settingsFieldErrors = [];
      this.settingsNotice = '';
      try {
        const response = await fetch('/dashboard/config', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json', ...authHeaders() },
          body: JSON.stringify(this.settingsPayload()),
        });
        const payload = await response.json().catch(() => ({}));
        if (response.status === 422) {
          this.settingsFieldErrors = payload.errors ?? [];
          throw new Error('Validation failed');
        }
        if (!response.ok || payload.ok === false) {
          throw new Error(payload.reason || `HTTP ${response.status}`);
        }
        this.settingsNotice = 'Saved. Restart required to apply.';
        return true;
      } catch (error) {
        this.settingsError = String(error);
        return false;
      } finally {
        this.settingsSaving = false;
      }
    },
    async restartServer() {
      this.settingsRestarting = true;
      this.settingsError = null;
      this.settingsNotice = '';
      try {
        const response = await fetch('/dashboard/restart', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json', ...authHeaders() },
        });
        const payload = await response.json().catch(() => ({}));
        if (payload.restarting) {
          this.settingsNotice = 'Restarting server…';
          await this.waitForServerBack();
        } else if (payload.manual) {
          this.settingsNotice = payload.instructions || 'Restart manually via the tray.';
        } else {
          throw new Error(payload.reason || payload.error || `HTTP ${response.status}`);
        }
      } catch (error) {
        this.settingsError = String(error);
      } finally {
        this.settingsRestarting = false;
      }
    },
    async waitForServerBack() {
      const deadline = Date.now() + 20000;
      // brief grace period so we poll the NEW process, not the dying one
      await new Promise((r) => window.setTimeout(r, 1500));
      while (Date.now() < deadline) {
        try {
          const response = await fetch('/healthz', { cache: 'no-store' });
          if (response.ok) {
            this.settingsNotice = 'Server restarted.';
            await this.loadSettings();
            return;
          }
        } catch (_) {
          /* server still down; keep polling */
        }
        await new Promise((r) => window.setTimeout(r, 750));
      }
      this.settingsError = 'Server did not come back within 20s; check the tray and logs.';
    },
    async saveAndRestart() {
      this.settingsConfirmOpen = false;
      if (await this.saveSettings()) {
        await this.restartServer();
      }
    },
    async copyIssueState() {
      await navigator.clipboard?.writeText(this.rawJson).catch(() => {});
      this.copiedIssueState = true;
      window.setTimeout(() => {
        this.copiedIssueState = false;
      }, 1600);
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
    requestBadgeClass(request) {
      return request.ok ? 'badge-success' : 'badge-error';
    },
    clientLabel(connectionId) {
      const client = this.connectedClients.find((item) => item.connection_id === connectionId);
      if (!client) return connectionId || 'unknown';
      const name = client.name || client.connection_id;
      return client.version ? `${name} ${client.version}` : name;
    },
    summaryPills(summary) {
      const pills = [];
      for (const section of ['args', 'result']) {
        const value = summary?.[section];
        if (!value || typeof value !== 'object' || Array.isArray(value)) continue;
        for (const [key, detail] of Object.entries(value)) {
          if (detail == null || detail === '') continue;
          pills.push(`${section}.${key}: ${compactValue(detail)}`);
        }
      }
      return pills.slice(0, 10);
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
    async recorderPost(path, body) {
      this.recorderBusy = true;
      this.recorderError = null;
      try {
        const response = await fetch(path, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json', ...authHeaders() },
          body: JSON.stringify(body ?? {}),
        });
        const data = await response.json().catch(() => ({}));
        if (!response.ok || data.ok === false) {
          throw new Error(data.error?.message ?? `HTTP ${response.status}`);
        }
        await this.loadState({ quiet: true });
        return data;
      } catch (error) {
        this.recorderError = String(error);
        return null;
      } finally {
        this.recorderBusy = false;
      }
    },
    async recorderStart() {
      const title = this.recorderTitle.trim() || 'Recorded macro';
      const result = await this.recorderPost('/dashboard/recorder/start', {
        title,
        description: this.recorderDescription || null,
        tags: ['recorded'],
        app_identity: null,
        capture_input: true,
      });
      if (result?.session?.id) {
        this.selectedRecordingId = 'active';
      }
    },
    async recorderPause(paused) {
      await this.recorderPost('/dashboard/recorder/pause', {
        paused,
        reason: paused ? 'paused from dashboard' : 'resumed from dashboard',
      });
    },
    async recorderStop() {
      const result = await this.recorderPost('/dashboard/recorder/stop', {
        save_to_memory: false,
      });
      if (result?.session?.id) {
        this.selectedRecordingId = result.session.id;
      }
    },
    async recorderPromote() {
      const sessionId =
        this.selectedRecording?.id === 'active' ? null : this.selectedRecording?.session?.id;
      await this.recorderPost('/dashboard/recorder/promote', {
        session_id: sessionId,
        remember: this.recorderRemember,
      });
    },
    recorderManifestRows(manifest) {
      if (!manifest) return [];
      return [
        ['Title', manifest.title],
        ['Version', manifest.version],
        ['Launch', manifest.launch?.tool],
        ['Launch exe', manifest.launch?.args?.exe],
        ['Bind strategy', manifest.bind?.strategy],
        ['Required executable', manifest.bind?.required_executable],
        ['Review required', manifest.replay?.extra?.recorder?.launch_inference?.requires_confirmation],
      ].filter(([, value]) => value != null && value !== '');
    },
    recorderStepTarget(step) {
      const target = step.target;
      if (!target) return 'n/a';
      if (target.type === 'uia_element') {
        return target.automation_id || target.name || target.class_name || target.element_ref || 'UIA element';
      }
      if (target.type === 'current') return 'current target';
      if (target.type === 'alias') return target.name;
      return target.type || 'target';
    },
    recorderStepArgs(step) {
      const args = step.args ?? {};
      if (step.tool === 'input.type_text') return 'text redacted';
      if (step.tool === 'macro.type_secret') return 'secret placeholder';
      if (step.tool?.startsWith('input.')) {
        return Object.entries(args)
          .filter(([key]) => !['text', 'value'].includes(key))
          .map(([key, value]) => `${key}: ${value}`)
          .join(', ');
      }
      return compactValue(args);
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
      this.expandDocGroupForSlug(slug);
      this.renderMermaid();
    },
    docGroupForSlug(slug) {
      const doc = this.docs.find((candidate) => candidate.slug === slug);
      return doc ? docGroupFor(doc) : null;
    },
    expandDocGroupForSlug(slug) {
      const group = this.docGroupForSlug(slug);
      if (!group) return;
      this.collapsedDocGroups = {
        ...this.collapsedDocGroups,
        [group.id]: false,
      };
    },
    docGroupIsOpen(group) {
      if (this.docsSearch.trim()) return true;
      const stored = this.collapsedDocGroups[group.id];
      if (typeof stored === 'boolean') return !stored;
      if (group.kind === 'project') return true;
      return group.docs.some((doc) => doc.slug === this.activeDocSlug);
    },
    setDocGroupOpen(group, event) {
      if (this.docsSearch.trim()) return;
      const open = Boolean(event?.target?.open);
      this.collapsedDocGroups = {
        ...this.collapsedDocGroups,
        [group.id]: !open,
      };
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
        <nav class="mb-5 flex flex-wrap border-b border-slate-200 dark:border-slate-700" aria-label="Dashboard sections">
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

        <section v-if="selectedTab === 'observability'" class="space-y-5">
          <section class="winctl-card overflow-hidden">
            <div class="winctl-card-header flex items-center justify-between gap-3 px-4 py-3">
              <h2 class="text-sm font-semibold">Connected clients</h2>
              <span class="badge badge-info badge-outline">{{ connectedClients.length }}</span>
            </div>
            <div class="winctl-table-toolbar">
              <input
                v-model="tableState.clients.query"
                type="search"
                placeholder="Search clients"
                class="input input-sm input-bordered w-full sm:max-w-xs"
                @input="resetTablePage('clients')"
              />
              <span class="text-xs text-slate-500">
                Showing {{ tableMeta('clients', connectedClients, clientTableFields).start }}-{{ tableMeta('clients', connectedClients, clientTableFields).end }}
                of {{ tableMeta('clients', connectedClients, clientTableFields).filtered }}
              </span>
            </div>
            <div class="winctl-table-frame">
              <table class="table winctl-table table-sm">
                <thead>
                  <tr><th>Client</th><th>Transport</th><th>Connected</th><th>Last seen</th><th>Requests</th><th>Last tool</th></tr>
                </thead>
                <tbody>
                  <tr v-if="!tableMeta('clients', connectedClients, clientTableFields).filtered">
                    <td colspan="6" class="py-8 text-center text-slate-500">{{ tableState.clients.query ? 'No matching clients' : 'No connected clients' }}</td>
                  </tr>
                  <tr v-for="client in tablePageRows('clients', connectedClients, clientTableFields)" :key="client.connection_id">
                    <td>
                      <div class="font-medium">{{ client.name || 'Uninitialized client' }}</div>
                      <div class="winctl-code text-xs text-slate-500">{{ client.connection_id }}</div>
                      <div class="text-xs text-slate-500">{{ client.version || 'unknown version' }}</div>
                    </td>
                    <td><span class="badge badge-ghost badge-sm">{{ client.transport }}</span></td>
                    <td class="text-xs">{{ formatUnixMs(client.connected_at_unix_ms) }}</td>
                    <td class="text-xs">{{ formatUnixMs(client.last_seen_unix_ms) }}</td>
                    <td class="text-sm font-semibold">{{ client.request_count || 0 }}</td>
                    <td class="winctl-code text-xs">{{ client.last_tool || 'n/a' }}</td>
                  </tr>
                </tbody>
              </table>
            </div>
            <div class="winctl-table-footer">
              <button type="button" class="btn btn-xs" :disabled="tableMeta('clients', connectedClients, clientTableFields).page <= 1" @click="previousTablePage('clients', connectedClients, clientTableFields)">Previous</button>
              <span class="text-xs text-slate-500">Page {{ tableMeta('clients', connectedClients, clientTableFields).page }} of {{ tableMeta('clients', connectedClients, clientTableFields).totalPages }}</span>
              <button type="button" class="btn btn-xs" :disabled="tableMeta('clients', connectedClients, clientTableFields).page >= tableMeta('clients', connectedClients, clientTableFields).totalPages" @click="nextTablePage('clients', connectedClients, clientTableFields)">Next</button>
            </div>
          </section>

          <section class="winctl-card overflow-hidden">
            <div class="winctl-card-header flex items-center justify-between gap-3 px-4 py-3">
              <h2 class="text-sm font-semibold">Recent requests</h2>
              <span class="badge badge-info badge-outline">{{ recentRequests.length }}</span>
            </div>
            <div class="winctl-table-toolbar">
              <input
                v-model="tableState.requests.query"
                type="search"
                placeholder="Search requests"
                class="input input-sm input-bordered w-full sm:max-w-xs"
                @input="resetTablePage('requests')"
              />
              <span class="text-xs text-slate-500">
                Showing {{ tableMeta('requests', recentRequests, requestTableFields).start }}-{{ tableMeta('requests', recentRequests, requestTableFields).end }}
                of {{ tableMeta('requests', recentRequests, requestTableFields).filtered }}
              </span>
            </div>
            <div class="winctl-table-frame">
              <table class="table winctl-table table-sm">
                <thead>
                  <tr><th>Time</th><th>Client</th><th>Tool</th><th>Duration</th><th>Status</th><th>Summary</th></tr>
                </thead>
                <tbody>
                  <tr v-if="!tableMeta('requests', recentRequests, requestTableFields).filtered">
                    <td colspan="6" class="py-8 text-center text-slate-500">{{ tableState.requests.query ? 'No matching requests' : 'No tool requests recorded' }}</td>
                  </tr>
                  <tr v-for="request in tablePageRows('requests', recentRequests, requestTableFields)" :key="request.id">
                    <td class="text-xs">{{ formatUnixMs(request.started_at_unix_ms) }}</td>
                    <td>
                      <div class="text-xs font-medium">{{ clientLabel(request.connection_id) }}</div>
                      <div class="winctl-code text-[11px] text-slate-500">{{ request.connection_id }}</div>
                    </td>
                    <td class="winctl-code text-xs">{{ request.tool_name }}</td>
                    <td class="text-xs">{{ request.duration_ms }} ms</td>
                    <td>
                      <span class="badge badge-sm" :class="requestBadgeClass(request)">{{ request.ok ? 'ok' : 'failed' }}</span>
                      <div v-if="request.error_code" class="winctl-code mt-1 text-[11px] text-slate-500">{{ request.error_code }}</div>
                    </td>
                    <td>
                      <div class="flex flex-wrap gap-1">
                        <span v-for="pill in summaryPills(request.summary)" :key="pill" class="badge badge-ghost badge-sm">{{ pill }}</span>
                        <span v-if="!summaryPills(request.summary).length" class="text-xs text-slate-500">No redacted fields</span>
                      </div>
                    </td>
                  </tr>
                </tbody>
              </table>
            </div>
            <div class="winctl-table-footer">
              <button type="button" class="btn btn-xs" :disabled="tableMeta('requests', recentRequests, requestTableFields).page <= 1" @click="previousTablePage('requests', recentRequests, requestTableFields)">Previous</button>
              <span class="text-xs text-slate-500">Page {{ tableMeta('requests', recentRequests, requestTableFields).page }} of {{ tableMeta('requests', recentRequests, requestTableFields).totalPages }}</span>
              <button type="button" class="btn btn-xs" :disabled="tableMeta('requests', recentRequests, requestTableFields).page >= tableMeta('requests', recentRequests, requestTableFields).totalPages" @click="nextTablePage('requests', recentRequests, requestTableFields)">Next</button>
            </div>
          </section>
        </section>

        <section v-if="selectedTab === 'connect'" class="space-y-5">
          <div class="grid gap-5 lg:grid-cols-[0.9fr_1.1fr]">
            <section class="winctl-card">
              <div class="winctl-card-header flex items-center justify-between gap-3 px-4 py-3">
                <h2 class="text-sm font-semibold">MCP endpoint</h2>
                <button type="button" class="btn btn-xs" :disabled="connectLoading" @click="loadConnect">Refresh</button>
              </div>
              <dl class="divide-y divide-slate-200 text-sm dark:divide-slate-700">
                <div class="grid grid-cols-[130px_1fr] gap-3 px-4 py-3">
                  <dt class="text-slate-500">URL</dt>
                  <dd class="flex min-w-0 items-center gap-2">
                    <code class="winctl-code min-w-0 break-all text-xs">{{ connectMcpUrl }}</code>
                    <button type="button" class="btn btn-xs shrink-0" @click="copyConnectText('mcp-url', connectMcpUrl)">
                      {{ copiedConnectId === 'mcp-url' ? 'Copied' : 'Copy' }}
                    </button>
                  </dd>
                </div>
                <div class="grid grid-cols-[130px_1fr] gap-3 px-4 py-3">
                  <dt class="text-slate-500">Auth</dt>
                  <dd>
                    <span class="badge badge-sm" :class="connectState?.auth_required ? 'badge-warning' : 'badge-ghost'">
                      {{ connectState?.auth_required ? 'Bearer token required' : 'Loopback auth optional' }}
                    </span>
                  </dd>
                </div>
                <div class="grid grid-cols-[130px_1fr] gap-3 px-4 py-3">
                  <dt class="text-slate-500">Dashboard token</dt>
                  <dd>
                    <div v-if="currentDashboardToken" class="flex min-w-0 flex-wrap items-center gap-2">
                      <code class="winctl-code max-w-full break-all rounded bg-base-200 px-2 py-1 text-xs">
                        {{ connectDashboardTokenVisible ? currentDashboardToken : '••••••••••••••••••••••••' }}
                      </code>
                      <button type="button" class="btn btn-xs" @click="connectDashboardTokenVisible = !connectDashboardTokenVisible">
                        {{ connectDashboardTokenVisible ? 'Hide' : 'Show' }}
                      </button>
                      <button type="button" class="btn btn-xs" @click="copyConnectText('dashboard-token', currentDashboardToken)">
                        {{ copiedConnectId === 'dashboard-token' ? 'Copied' : 'Copy' }}
                      </button>
                    </div>
                    <span v-else class="text-xs text-slate-500">No dashboard token in this browser session</span>
                  </dd>
                </div>
              </dl>
            </section>

            <section class="winctl-card overflow-hidden">
              <div class="winctl-card-header flex items-center justify-between gap-3 px-4 py-3">
                <h2 class="text-sm font-semibold">Tokens</h2>
                <span class="badge badge-info badge-outline">{{ connectTokens.length }}</span>
              </div>
              <div class="space-y-4 p-4">
                <div class="flex flex-col gap-2 sm:flex-row">
                  <input
                    v-model="connectTokenLabel"
                    type="text"
                    class="input input-sm input-bordered flex-1"
                    placeholder="Token label"
                    @keyup.enter="createConnectToken"
                  />
                  <button type="button" class="btn btn-sm btn-info" :disabled="connectTokenBusy" @click="createConnectToken">Add token</button>
                </div>
                <div v-if="connectState && !connectState.auth_required" class="alert alert-warning text-sm">
                  <span>This loopback server is not enforcing tokens. Restart HTTP with <code class="winctl-code">--auth-token</code> to require bearer auth from startup.</span>
                </div>
                <div v-if="connectError" class="alert alert-error text-sm">{{ connectError }}</div>
                <div v-if="connectTokenValue" class="rounded-lg border border-[#06d6a0]/40 bg-[#06d6a0]/10 p-3">
                  <div class="mb-2 flex items-center justify-between gap-3">
                    <span class="text-sm font-semibold">{{ connectTokenMetadata?.label || 'Token' }}</span>
                    <button type="button" class="btn btn-xs" @click="copyConnectText('revealed-token', connectTokenValue)">
                      {{ copiedConnectId === 'revealed-token' ? 'Copied' : 'Copy token' }}
                    </button>
                  </div>
                  <code class="winctl-code block break-all text-xs">{{ connectTokenValue }}</code>
                </div>
                <div class="winctl-table-frame rounded-lg border border-slate-200 dark:border-slate-700">
                  <table class="table winctl-table table-sm">
                    <thead>
                      <tr><th>Label</th><th>Created</th><th>Last used</th><th>Uses</th><th>Actions</th></tr>
                    </thead>
                    <tbody>
                      <tr v-if="!connectTokens.length">
                        <td colspan="5" class="py-8 text-center text-slate-500">No active tokens</td>
                      </tr>
                      <tr v-for="token in connectTokens" :key="token.id">
                        <td>
                          <div class="font-medium">{{ token.label }}</div>
                          <div class="winctl-code text-xs text-slate-500">{{ token.id }}</div>
                          <span v-if="token.startup" class="badge badge-xs badge-ghost">startup</span>
                        </td>
                        <td class="text-xs">{{ formatUnixMs(token.created_at_unix_ms) }}</td>
                        <td class="text-xs">{{ formatUnixMs(token.last_used_at_unix_ms) }}</td>
                        <td class="text-sm font-semibold">{{ token.use_count || 0 }}</td>
                        <td>
                          <div class="flex flex-wrap gap-1">
                            <button type="button" class="btn btn-xs" :disabled="connectTokenBusy" @click="revealConnectToken(token)">Reveal</button>
                            <button type="button" class="btn btn-xs btn-ghost text-error" :disabled="connectTokenBusy" @click="revokeConnectToken(token)">Revoke</button>
                          </div>
                        </td>
                      </tr>
                    </tbody>
                  </table>
                </div>
              </div>
            </section>
          </div>

          <section class="winctl-card">
            <div class="winctl-card-header flex flex-col gap-3 px-4 py-3 sm:flex-row sm:items-center sm:justify-between">
              <div>
                <h2 class="text-sm font-semibold">Client setup</h2>
                <div class="text-xs text-slate-500">
                  {{ connectMode === 'http' ? 'HTTP connects clients to this running dashboard server.' : 'Stdio starts a separate MCP server process; dashboard features remain on HTTP.' }}
                </div>
              </div>
              <div class="join">
                <button
                  type="button"
                  class="btn btn-sm join-item"
                  :class="connectMode === 'http' ? 'btn-info' : 'btn-ghost'"
                  @click="connectMode = 'http'"
                >HTTP recommended</button>
                <button
                  type="button"
                  class="btn btn-sm join-item"
                  :class="connectMode === 'stdio' ? 'btn-info' : 'btn-ghost'"
                  @click="connectMode = 'stdio'"
                >Stdio fallback</button>
              </div>
            </div>
          </section>

          <div class="grid gap-5 xl:grid-cols-2">
            <section v-for="example in connectExamples" :key="example.id" class="winctl-card overflow-hidden">
              <div class="winctl-card-header flex items-center justify-between gap-3 px-4 py-3">
                <div>
                  <h2 class="text-sm font-semibold">{{ example.title }}</h2>
                  <div class="winctl-code text-xs text-slate-500">{{ example.path }}</div>
                </div>
                <div class="flex gap-1">
                  <button type="button" class="btn btn-xs" @click="copyConnectText(example.id + ':config', example.config)">
                    {{ copiedConnectId === example.id + ':config' ? 'Copied' : 'Copy config' }}
                  </button>
                  <button type="button" class="btn btn-xs" @click="copyConnectText(example.id + ':command', example.command)">
                    {{ copiedConnectId === example.id + ':command' ? 'Copied' : 'Copy command' }}
                  </button>
                </div>
              </div>
              <div class="grid gap-3 p-4">
                <div>
                  <div class="mb-1 text-xs font-bold uppercase text-slate-500">Config</div>
                  <pre class="winctl-code overflow-x-auto rounded bg-[#0d1117] p-3 text-xs text-[#e6edf6]"><code>{{ example.config }}</code></pre>
                </div>
                <div>
                  <div class="mb-1 text-xs font-bold uppercase text-slate-500">Command</div>
                  <pre class="winctl-code overflow-x-auto rounded bg-[#0d1117] p-3 text-xs text-[#e6edf6]"><code>{{ example.command }}</code></pre>
                </div>
              </div>
            </section>
          </div>
        </section>

        <section v-if="selectedTab === 'recorder'" class="space-y-5">
          <div class="grid gap-5 lg:grid-cols-[0.85fr_1.15fr]">
            <section class="winctl-card">
              <div class="winctl-card-header flex items-center justify-between gap-3 px-4 py-3">
                <h2 class="text-sm font-semibold">Recording controls</h2>
                <span class="badge" :class="recorderStatusBadgeClass">{{ recordingState.status || 'idle' }}</span>
              </div>
              <div class="space-y-4 p-4">
                <div class="grid gap-3 sm:grid-cols-2">
                  <label class="form-control">
                    <span class="label-text text-xs font-semibold">Title</span>
                    <input v-model="recorderTitle" type="text" class="input input-sm input-bordered" />
                  </label>
                  <label class="form-control">
                    <span class="label-text text-xs font-semibold">Description</span>
                    <input v-model="recorderDescription" type="text" class="input input-sm input-bordered" />
                  </label>
                </div>
                <div class="flex flex-wrap items-center gap-2">
                  <button type="button" class="btn btn-sm btn-info" :disabled="recorderBusy || recordingActive" @click="recorderStart">Start</button>
                  <button type="button" class="btn btn-sm" :disabled="recorderBusy || !recordingActive || recordingState.status === 'paused'" @click="recorderPause(true)">Pause</button>
                  <button type="button" class="btn btn-sm" :disabled="recorderBusy || !recordingActive || recordingState.status !== 'paused'" @click="recorderPause(false)">Resume</button>
                  <button type="button" class="btn btn-sm btn-warning" :disabled="recorderBusy || !recordingActive" @click="recorderStop">Stop</button>
                  <label class="label cursor-pointer gap-2">
                    <input v-model="recorderRemember" type="checkbox" class="checkbox checkbox-sm" />
                    <span class="label-text text-xs">Remember on promote</span>
                  </label>
                </div>
                <div v-if="recorderError" class="alert alert-error text-sm">{{ recorderError }}</div>
                <dl class="winctl-detail-grid">
                  <div class="winctl-detail-row">
                    <dt>Native provider</dt>
                    <dd>{{ recorderNativeCapture.provider || 'n/a' }}</dd>
                  </div>
                  <div class="winctl-detail-row">
                    <dt>Hooks</dt>
                    <dd>
                      <span class="badge badge-sm" :class="badgeClass(recorderNativeCapture.hooks_started)">{{ recorderNativeCapture.hooks_started ? 'started' : 'stopped' }}</span>
                      <span class="ml-2 badge badge-sm" :class="badgeClass(recorderNativeCapture.hotkeys_started)">{{ recorderNativeCapture.hotkeys_started ? 'hotkeys' : 'no hotkeys' }}</span>
                    </dd>
                  </div>
                  <div class="winctl-detail-row">
                    <dt>Events</dt>
                    <dd>{{ recorderNativeCapture.captured_event_count || 0 }} captured, {{ recorderNativeCapture.emitted_step_count || 0 }} steps, {{ recorderNativeCapture.ignored_event_count || 0 }} ignored</dd>
                  </div>
                  <div v-if="recorderNativeCapture.last_error" class="winctl-detail-row">
                    <dt>Last error</dt>
                    <dd>{{ recorderNativeCapture.last_error }}</dd>
                  </div>
                </dl>
              </div>
            </section>

            <section class="winctl-card">
              <div class="winctl-card-header flex items-center justify-between gap-3 px-4 py-3">
                <h2 class="text-sm font-semibold">Review manifest</h2>
                <span class="badge badge-info badge-outline">{{ recorderSessionChoices.length }}</span>
              </div>
              <div class="space-y-4 p-4">
                <select v-model="selectedRecordingId" class="select select-bordered w-full">
                  <option v-if="!recorderSessionChoices.length" value="active">No recording sessions</option>
                  <option v-for="choice in recorderSessionChoices" :key="choice.id" :value="choice.id">{{ choice.label }}</option>
                </select>
                <div v-if="!selectedRecording" class="py-8 text-center text-sm text-slate-500">Start or stop a recording to review a manifest</div>
                <template v-else>
                  <dl class="winctl-detail-grid">
                    <div v-for="[label, value] in recorderManifestRows(selectedRecordingManifest)" :key="label" class="winctl-detail-row">
                      <dt>{{ label }}</dt>
                      <dd>{{ value }}</dd>
                    </div>
                  </dl>
                  <div v-if="recorderInference" class="rounded-lg border border-[#ffb454]/40 bg-[#ffb454]/10 p-3 text-sm">
                    <div class="font-semibold">Launch inference: {{ recorderInference.status || 'unknown' }}</div>
                    <div class="mt-1 text-xs text-slate-600 dark:text-slate-300">
                      Mode {{ recorderInference.mode || 'n/a' }}. Review is required before replay.
                    </div>
                  </div>
                  <div class="flex flex-wrap gap-2">
                    <button type="button" class="btn btn-sm" :disabled="!selectedRecordingManifest" @click="copyJson({ manifest: selectedRecordingManifest })">Copy manifest</button>
                    <button type="button" class="btn btn-sm btn-info" :disabled="recorderBusy || !selectedRecordingManifest" @click="recorderPromote">Promote</button>
                  </div>
                </template>
              </div>
            </section>
          </div>

          <section class="winctl-card overflow-hidden">
            <div class="winctl-card-header flex items-center justify-between gap-3 px-4 py-3">
              <h2 class="text-sm font-semibold">Recorded steps</h2>
              <span class="badge badge-info badge-outline">{{ selectedRecordingSteps.length }}</span>
            </div>
            <div class="winctl-table-frame">
              <table class="table winctl-table table-sm">
                <thead>
                  <tr><th>Step</th><th>Tool</th><th>Target</th><th>Arguments</th><th>Review</th></tr>
                </thead>
                <tbody>
                  <tr v-if="!selectedRecordingSteps.length">
                    <td colspan="5" class="py-8 text-center text-slate-500">No recorded steps</td>
                  </tr>
                  <tr v-for="step in selectedRecordingSteps" :key="step.id">
                    <td class="winctl-code text-xs">{{ step.id }}</td>
                    <td class="winctl-code text-xs">{{ step.tool }}</td>
                    <td class="text-xs">{{ recorderStepTarget(step) }}</td>
                    <td class="text-xs">{{ recorderStepArgs(step) }}</td>
                    <td>
                      <div class="flex flex-wrap gap-1">
                        <span v-if="step.tool === 'input.type_text'" class="badge badge-warning badge-outline badge-xs">scrub text</span>
                        <span v-if="step.tool === 'macro.type_secret'" class="badge badge-warning badge-outline badge-xs">bind secret</span>
                        <span v-if="step.coordinate_fallback" class="badge badge-ghost badge-xs">coordinate fallback</span>
                      </div>
                    </td>
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

        <section v-if="selectedTab === 'docs'" class="winctl-docs-layout grid gap-5 lg:grid-cols-[320px_1fr]">
          <aside class="winctl-card winctl-docs-card winctl-docs-sidebar overflow-hidden">
            <div class="winctl-card-header px-4 py-3">
              <h2 class="text-sm font-semibold">Documentation</h2>
            </div>
            <div class="winctl-docs-sidebar-body p-3 space-y-3">
              <input
                v-model="docsSearch"
                type="search"
                placeholder="Search docs"
                class="input input-sm input-bordered w-full"
              />
              <div v-if="docsLoading" class="px-1 text-sm opacity-70">Loading docs…</div>
              <div v-else-if="docsError" class="px-1 text-sm text-error">{{ docsError }}</div>
              <ul v-else class="menu menu-sm menu-vertical winctl-doc-menu bg-base-200 rounded-box w-full" aria-label="Documentation navigation">
                <li v-for="group in filteredDocGroups" :key="group.id">
                  <details :open="docGroupIsOpen(group)" @toggle="setDocGroupOpen(group, $event)">
                    <summary>
                      <span class="truncate">{{ group.label }}</span>
                      <span class="badge badge-xs badge-ghost">{{ group.docs.length }}</span>
                    </summary>
                    <ul>
                      <li v-for="doc in group.docs" :key="doc.slug">
                        <a
                          :class="{ 'menu-active': doc.slug === activeDocSlug }"
                          @click="selectDoc(doc.slug)"
                        >{{ doc.title }}</a>
                      </li>
                    </ul>
                  </details>
                </li>
                <li v-if="!filteredDocs.length" class="px-2 py-1 text-sm opacity-60">No matches</li>
              </ul>
            </div>
          </aside>
          <section class="winctl-card winctl-docs-card overflow-hidden">
            <div class="winctl-card-header px-4 py-3">
              <h2 class="text-sm font-semibold">{{ activeDoc?.title ?? 'Select a document' }}</h2>
            </div>
            <div
              class="winctl-markdown winctl-doc-content p-5"
              @click="handleDocClick"
              v-html="activeDoc?.html ?? '<p class=&quot;opacity-60&quot;>Choose a tool doc from the list.</p>'"
            ></div>
          </section>
        </section>

        <section v-if="selectedTab === 'config'" class="space-y-5">
          <section class="winctl-card">
            <div class="winctl-card-header flex flex-col gap-3 px-4 py-3 sm:flex-row sm:items-center sm:justify-between">
              <div>
                <h2 class="text-sm font-semibold">Server settings</h2>
                <div class="text-xs text-slate-500">Edits the server's config file; a restart applies them.</div>
              </div>
              <button type="button" class="btn btn-xs" :disabled="settingsLoading" @click="loadSettings">Refresh</button>
            </div>

            <div v-if="!settingsEditable" class="alert alert-warning mx-4 mt-4 text-xs">
              Server started without a <code>--config</code> file; settings are read-only.
            </div>
            <div v-if="settingsError" class="alert alert-error mx-4 mt-4 text-sm">{{ settingsError }}</div>
            <div v-if="settingsNotice" class="alert alert-info mx-4 mt-4 text-sm">{{ settingsNotice }}</div>

            <dl class="grid gap-2 px-4 py-3 text-xs sm:grid-cols-2">
              <div><dt class="font-semibold">Transport</dt><dd>{{ settingsConnection.transport }}</dd></div>
              <div><dt class="font-semibold">Listen</dt><dd>{{ settingsConnection.listen }}</dd></div>
              <div><dt class="font-semibold">Auth required</dt><dd>{{ settingsConnection.auth_required ? 'yes' : 'no' }}</dd></div>
              <div><dt class="font-semibold">Auth token set</dt><dd>{{ settingsConnection.auth_token_set ? 'yes' : 'no' }}</dd></div>
            </dl>
          </section>

          <fieldset :disabled="!settingsEditable" class="min-w-0 space-y-5">
            <section class="winctl-card p-4">
              <h3 class="mb-3 text-sm font-semibold">Policy</h3>
              <div class="grid gap-2 sm:grid-cols-2">
                <label class="label cursor-pointer gap-2"><input v-model="settingsForm.policy.enable_filesystem_mutation" type="checkbox" class="checkbox checkbox-sm" /><span class="label-text text-xs">Filesystem mutation</span></label>
                <label class="label cursor-pointer gap-2"><input v-model="settingsForm.policy.enable_clipboard_write" type="checkbox" class="checkbox checkbox-sm" /><span class="label-text text-xs">Clipboard write</span></label>
                <label class="label cursor-pointer gap-2"><input v-model="settingsForm.policy.enable_registry_mutation" type="checkbox" class="checkbox checkbox-sm" /><span class="label-text text-xs">Registry mutation</span></label>
                <label class="label cursor-pointer gap-2"><input v-model="settingsForm.policy.allow_private_network" type="checkbox" class="checkbox checkbox-sm" /><span class="label-text text-xs">Allow private network</span></label>
                <label class="label cursor-pointer gap-2"><input v-model="settingsForm.policy.memory_mutation_enabled" type="checkbox" class="checkbox checkbox-sm" /><span class="label-text text-xs">Memory mutation</span></label>
              </div>
              <div class="mt-3 grid gap-3 sm:grid-cols-3">
                <label class="form-control"><span class="label-text text-xs font-semibold">Max macro runtime (ms)</span><input v-model="settingsForm.policy.max_macro_runtime_ms" type="number" min="0" class="input input-sm input-bordered" /></label>
                <label class="form-control"><span class="label-text text-xs font-semibold">Max macro steps</span><input v-model="settingsForm.policy.max_macro_steps" type="number" min="0" class="input input-sm input-bordered" /></label>
                <label class="form-control"><span class="label-text text-xs font-semibold">Screenshot retention</span><input v-model="settingsForm.policy.screenshot_retention_count" type="number" min="0" class="input input-sm input-bordered" /></label>
              </div>
              <div class="mt-3 grid gap-3 sm:grid-cols-2">
                <label class="form-control"><span class="label-text text-xs font-semibold">Tool allowlist (one per line)</span><textarea v-model="settingsForm.policy.tool_allowlist" rows="3" class="textarea textarea-bordered textarea-sm"></textarea></label>
                <label class="form-control"><span class="label-text text-xs font-semibold">Tool denylist (one per line)</span><textarea v-model="settingsForm.policy.tool_denylist" rows="3" class="textarea textarea-bordered textarea-sm"></textarea></label>
              </div>
            </section>

            <section class="winctl-card p-4">
              <h3 class="mb-3 text-sm font-semibold">Paths</h3>
              <div class="grid gap-3 sm:grid-cols-3">
                <label class="form-control"><span class="label-text text-xs font-semibold">Capture dir</span><input v-model="settingsForm.paths.capture_dir" type="text" class="input input-sm input-bordered" /></label>
                <label class="form-control"><span class="label-text text-xs font-semibold">Artifact dir</span><input v-model="settingsForm.paths.artifact_dir" type="text" class="input input-sm input-bordered" /></label>
                <label class="form-control"><span class="label-text text-xs font-semibold">Memory DB</span><input v-model="settingsForm.paths.memory_db" type="text" class="input input-sm input-bordered" /></label>
              </div>
              <label class="form-control mt-3"><span class="label-text text-xs font-semibold">Filesystem roots (one per line)</span><textarea v-model="settingsForm.paths.filesystem_roots" rows="3" class="textarea textarea-bordered textarea-sm"></textarea></label>
            </section>

            <section class="winctl-card p-4">
              <h3 class="mb-3 text-sm font-semibold">Logging &amp; embedding</h3>
              <div class="grid gap-3 sm:grid-cols-3">
                <label class="form-control"><span class="label-text text-xs font-semibold">Log file</span><input v-model="settingsForm.logging.log_file" type="text" class="input input-sm input-bordered" /></label>
                <label class="form-control"><span class="label-text text-xs font-semibold">Embedding model path</span><input v-model="settingsForm.embedding.model_path" type="text" class="input input-sm input-bordered" /></label>
                <label class="form-control"><span class="label-text text-xs font-semibold">Embedding dimension</span><input v-model="settingsForm.embedding.dimension" type="number" min="1" class="input input-sm input-bordered" /></label>
              </div>
            </section>

            <section class="winctl-card p-4">
              <h3 class="mb-3 text-sm font-semibold">Macro execution</h3>
              <div class="grid gap-2 sm:grid-cols-2">
                <label class="label cursor-pointer gap-2"><input v-model="settingsForm.macro_execution.enabled" type="checkbox" class="checkbox checkbox-sm" /><span class="label-text text-xs">Enabled</span></label>
                <label class="label cursor-pointer gap-2"><input v-model="settingsForm.macro_execution.allow_destructive_tools" type="checkbox" class="checkbox checkbox-sm" /><span class="label-text text-xs">Allow destructive tools</span></label>
              </div>
              <div class="mt-3 grid gap-3 sm:grid-cols-2">
                <label class="form-control"><span class="label-text text-xs font-semibold">Max runtime (ms)</span><input v-model="settingsForm.macro_execution.max_runtime_ms" type="number" min="0" class="input input-sm input-bordered" /></label>
                <label class="form-control"><span class="label-text text-xs font-semibold">Max steps</span><input v-model="settingsForm.macro_execution.max_steps" type="number" min="0" class="input input-sm input-bordered" /></label>
              </div>
            </section>

            <ul v-if="settingsFieldErrors.length" class="alert alert-error text-xs">
              <li v-for="err in settingsFieldErrors" :key="err.field"><strong>{{ err.field }}</strong>: {{ err.message }}</li>
            </ul>

            <div class="flex flex-wrap gap-2">
              <button type="button" class="btn btn-sm" :disabled="settingsSaving || !settingsEditable" @click="saveSettings">Save</button>
              <button type="button" class="btn btn-sm btn-info" :disabled="settingsSaving || settingsRestarting || !settingsEditable" @click="settingsConfirmOpen = true">Save &amp; Restart</button>
            </div>
          </fieldset>

          <div v-if="settingsConfirmOpen" class="modal modal-open">
            <div class="modal-box">
              <h3 class="text-sm font-semibold">Restart the server?</h3>
              <p class="py-2 text-xs">This saves your changes and restarts the MCP server via the tray. The dashboard will reconnect when it's back.</p>
              <p v-if="!settingsTrayAvailable" class="text-xs text-warning">Tray binary not found next to the server — you'll get manual restart instructions instead.</p>
              <div class="modal-action">
                <button type="button" class="btn btn-sm" @click="settingsConfirmOpen = false">Cancel</button>
                <button type="button" class="btn btn-sm btn-info" @click="saveAndRestart">Save &amp; Restart</button>
              </div>
            </div>
            <form method="dialog" class="modal-backdrop" @click.prevent="settingsConfirmOpen = false">
              <button type="button">close</button>
            </form>
          </div>
        </section>

        <button
          v-if="selectedTab === 'overview'"
          type="button"
          class="btn btn-circle btn-info winctl-issue-help"
          aria-label="Open issue helper"
          @click="issuePanelOpen = true"
        >?</button>

        <div v-if="issuePanelOpen" class="modal modal-open">
          <div class="modal-box max-w-4xl">
            <h2 class="text-lg font-semibold">GitHub issue helper</h2>
            <p class="mt-2 text-sm text-slate-600 dark:text-slate-300">
              Open an issue on GitHub and include this dashboard state JSON when reporting dashboard or server behavior.
            </p>
            <div class="mt-4 flex flex-wrap gap-2">
              <a class="btn btn-sm btn-info" :href="issueUrl" target="_blank" rel="noreferrer">Open GitHub issue</a>
              <button type="button" class="btn btn-sm" @click="copyIssueState">
                {{ copiedIssueState ? 'Copied' : 'Copy state JSON' }}
              </button>
              <button type="button" class="btn btn-sm btn-ghost" @click="issuePanelOpen = false">Close</button>
            </div>
            <pre class="winctl-code winctl-issue-state mt-4 rounded bg-[#0d1117] p-3 text-xs text-[#e6edf6]"><code>{{ rawJson }}</code></pre>
          </div>
          <form method="dialog" class="modal-backdrop" @click.prevent="issuePanelOpen = false">
            <button type="button">close</button>
          </form>
        </div>
      </main>
    </div>
  `,
}).mount('#app');
