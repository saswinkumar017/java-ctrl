export type Timestamp = number;

export interface JdkTool {
  name: string;
  path: string | null;
  available: boolean;
}

export interface JdkInfo {
  id: string;
  javaHome: string;
  javaExecutable: string;
  javacExecutable: string | null;
  version: string | null;
  versionMajor: number | null;
  vendor: string | null;
  vmName: string | null;
  architecture: string | null;
  osArch: string | null;
  source: string;
  isDefault: boolean;
  tools: JdkTool[];
}

export interface JdkScan {
  jdks: JdkInfo[];
  defaultJavaHome: string | null;
  scannedAt: Timestamp;
  notes: string[];
}

export interface JvmProcess {
  pid: number;
  name: string;
  displayName: string;
  mainClass: string | null;
  mainJar: string | null;
  javaHome: string | null;
  javaVersion: string | null;
  vendor: string | null;
  commandLine: string;
  args: string[];
  jvmArgs: string[];
  appArgs: string[];
  uptimeSecs: number;
  startTime: number;
  cpuPercent: number;
  rssBytes: number;
  virtualBytes: number;
  executable: string | null;
  cwd: string | null;
  springPorts: number[];
  attachable: boolean;
}

export interface RuntimeDetails {
  pid: number;
  properties: Record<string, string>;
  jvmArgs: string[];
  javaCommand: string | null;
  classPath: string | null;
  launcherType: string | null;
  flags: string[];
  versionLines: string[];
  uptimeSecs: number;
  jfrRecordings: JfrRecording[];
  notes: string[];
}

export interface GcCollectorStat {
  index: number;
  name: string;
  invocations: number;
  timeMs: number;
  lastPauseMs: number;
}

export interface MemoryPool {
  name: string;
  kind: string;
  used: number;
  committed: number;
  max: number;
}

export interface PerfRuntime {
  javaVersion: string | null;
  javaHome: string | null;
  vmName: string | null;
  vmVendor: string | null;
  vmVersion: string | null;
  vmInfo: string | null;
  javaCommand: string | null;
  vmArgs: string | null;
  classPath: string | null;
}

export interface MonitorSample {
  pid: number;
  timestamp: Timestamp;
  cpuPercent: number;
  rssBytes: number;
  virtualBytes: number;
  uptimeSecs: number;
  heapUsed: number;
  heapCommitted: number;
  heapMax: number;
  nonHeapUsed: number;
  nonHeapCommitted: number;
  edenUsed: number;
  edenCommitted: number;
  survivorUsed: number;
  survivorCommitted: number;
  oldUsed: number;
  oldCommitted: number;
  metaspaceUsed: number;
  metaspaceCommitted: number;
  metaspaceMax: number;
  compressedClassSpaceUsed: number;
  compressedClassSpaceCommitted: number;
  compressedClassSpaceMax: number;
  codeCacheUsed: number;
  codeCacheCommitted: number;
  codeCacheMax: number;
  threadsLive: number;
  threadsDaemon: number;
  threadsPeak: number;
  threadsStarted: number;
  classesLoaded: number;
  classesUnloaded: number;
  classesCurrent: number;
  gcCount: number;
  gcTimeMs: number;
  gcLastCause: string | null;
  gcCollectors: GcCollectorStat[];
  jitCompiles: number;
  jitTimeMs: number;
  jitBailouts: number;
  jitInvalidates: number;
  jitNmethods: number;
  jitNmethodSize: number;
  memoryPools: MemoryPool[];
  runtime: PerfRuntime | null;
  attachError: string | null;
  source: string;
}

export interface SamplePoint {
  timestamp: Timestamp;
  cpuPercent: number;
  heapUsed: number;
  heapCommitted: number;
  heapMax: number;
  nonHeapUsed: number;
  metaspaceUsed: number;
  codeCacheUsed: number;
  threadsLive: number;
  classesCurrent: number;
  gcCount: number;
  gcTimeMs: number;
  jitCompiles: number;
  jitTimeMs: number;
}

export interface GcEvent {
  id: number;
  pid: number;
  timestamp: Timestamp;
  collector: string;
  invocations: number;
  durationMs: number;
  heapBefore: number;
  heapAfter: number;
  reclaimedBytes: number;
  cause: string | null;
}

export interface ThreadInfo {
  name: string;
  number: number | null;
  daemon: boolean;
  priority: number | null;
  osPriority: number | null;
  cpuMs: number;
  elapsedSecs: number;
  tid: string | null;
  nid: string | null;
  state: string;
  stateDetail: string | null;
  header: string;
  stack: string[];
  locks: string[];
}

export interface ThreadSnapshot {
  pid: number;
  timestamp: Timestamp;
  raw: string;
  threads: ThreadInfo[];
  counts: Record<string, number>;
  deadlock: boolean;
  deadlockText: string | null;
  total: number;
  generatedAt: string | null;
}

export interface ClassLoaderInfo {
  id: string;
  parentId: string;
  cld: string;
  classes: number;
  chunkSize: number;
  blockSize: number;
  kind: string;
  hiddenClasses: number;
}

export interface ClassHistogramRow {
  rank: number;
  instances: number;
  bytes: number;
  className: string;
  module: string | null;
}

export interface ClassLoadingSnapshot {
  pid: number;
  timestamp: Timestamp;
  loaders: ClassLoaderInfo[];
  totalClasses: number;
  totalChunks: number;
  totalBlocks: number;
  histogram: ClassHistogramRow[];
  totalInstances: number;
  totalBytes: number;
  notes: string[];
}

export interface CodeHeapInfo {
  name: string;
  size: number;
  used: number;
  maxUsed: number;
  free: number;
}

export interface CompiledMethod {
  id: number;
  level: number;
  sync: number;
  method: string;
  addressRange: string;
}

export interface JitSnapshot {
  pid: number;
  timestamp: Timestamp;
  compiles: number;
  timeMs: number;
  bailouts: number;
  invalidates: number;
  nmethods: number;
  nmethodCodeSize: number;
  codeHeaps: CodeHeapInfo[];
  codeTotal: number;
  codeUsed: number;
  codeMaxUsed: number;
  codeFree: number;
  compilationEnabled: boolean;
  compiledMethods: CompiledMethod[];
  totalCompiledMethods: number;
  compilerThreads: number;
  queue: string[];
  notes: string[];
}

export interface JfrRecording {
  id: number;
  name: string | null;
  state: string | null;
  duration: string | null;
  destination: string | null;
  raw: string;
}

export interface DumpInfo {
  id: string;
  pid: number | null;
  kind: string;
  path: string;
  createdAt: Timestamp;
  sizeBytes: number;
  summary: Record<string, unknown> | null;
}

export interface TerminationResult {
  pid: number;
  force: boolean;
  status: string;
  message: string;
}

export interface SpringCandidate {
  pid: number;
  port: number;
  url: string;
  detectedFrom: string;
  name: string;
  reachable: boolean;
  detail: string | null;
}

export interface SpringMeasurement {
  statistic: string;
  value: number;
}

export interface SpringMetric {
  name: string;
  description: string | null;
  baseUnit: string | null;
  measurements: SpringMeasurement[];
  availableTags: string[];
}

export interface SpringSnapshot {
  baseUrl: string;
  checkedAt: Timestamp;
  health: unknown;
  info: unknown;
  links: unknown;
  metrics: SpringMetric[];
  latencyMs: number;
  notes: string[];
}

export interface SpringConnection {
  id: number;
  name: string;
  baseUrl: string;
  username: string | null;
  createdAt: Timestamp;
  lastCheckedAt: Timestamp | null;
}

export interface HeapClassRow {
  className: string;
  instances: number;
  bytes: number;
}

export interface HeapObjectRow {
  objectId: string;
  className: string;
  shallowBytes: number;
  retainedBytes: number | null;
}

export interface HeapLoaderRow {
  loader: string;
  objects: number;
  bytes: number;
  classes: number;
}

export interface RootCountRow {
  kind: string;
  count: number;
}

export interface HeapAnalysis {
  path: string;
  fileSize: number;
  dumpTimestamp: Timestamp | null;
  idSize: number;
  totalObjects: number;
  totalShallowBytes: number;
  classCount: number;
  reachableObjects: number;
  reachableBytes: number;
  weakObjects: number;
  weakBytes: number;
  classes: HeapClassRow[];
  weakClasses: HeapClassRow[];
  loaders: HeapLoaderRow[];
  topObjects: HeapObjectRow[];
  dominators: HeapObjectRow[];
  rootCounts: RootCountRow[];
  analysisMs: number;
  dominatorsComputed: boolean;
  notes: string[];
}

export interface AppInfo {
  version: string;
  dataDir: string;
  dumpsDir: string;
  platform: string;
  arch: string;
  javaHome: string | null;
}

export interface AppSettings {
  defaultJdk: string | null;
  refreshIntervalMs: number;
  retentionDays: number;
  sampleRetentionMinutes: number;
}
