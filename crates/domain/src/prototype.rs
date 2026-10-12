use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Policy {
    Fifo,
    RoundRobin,
    StrictPriority,
    PriorityAging,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Demo,
    Ollama,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    Deny,
    Ask,
    Allow,
}
impl Default for Permission {
    fn default() -> Self {
        Self::Deny
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryPolicy {
    pub max_entries: usize,
    pub max_chars: usize,
    pub ttl_ms: u64,
    pub share_outputs: bool,
}
impl Default for MemoryPolicy {
    fn default() -> Self {
        Self {
            max_entries: 32,
            max_chars: 8000,
            ttl_ms: 0,
            share_outputs: true,
        }
    }
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    Private,
    Shared,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: String,
    pub task_id: String,
    pub agent_id: String,
    pub kind: String,
    pub visibility: Visibility,
    pub at_ms: u64,
    pub text: String,
    pub truncated: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Document {
    pub id: String,
    pub name: String,
    pub bytes: u64,
    pub sha256: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DocumentRequest {
    pub id: String,
    pub document: Document,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentAccess {
    pub request: DocumentRequest,
    pub approval: Option<bool>,
    pub done: bool,
    pub revoked: bool,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Running,
    Paused,
    Stopping,
    Interrupted,
    Completed,
    Failed,
    Cancelled,
}
impl RunStatus {
    pub fn terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Ready,
    Running,
    Waiting,
    Approval,
    Completed,
    Failed,
    Cancelled,
}
impl TaskStatus {
    pub fn terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub mode: Mode,
    pub endpoint: String,
    pub policy: Policy,
    pub worker_slots: usize,
    pub memory_mb: u64,
    pub token_budget: u64,
    pub api_budget: u64,
    pub step_tokens: u64,
    pub output_tokens: u32,
    pub context_chars: usize,
    pub aging_quanta: u64,
    pub demo_delay_ms: u64,
    pub max_retries: u32,
    #[serde(default)]
    pub memory_policy: MemoryPolicy,
    #[serde(default = "default_tool_budget")]
    pub tool_budget: u64,
}
fn default_tool_budget() -> u64 {
    48
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            mode: Mode::Demo,
            endpoint: "http://127.0.0.1:11434".into(),
            policy: Policy::PriorityAging,
            worker_slots: 2,
            memory_mb: 1024,
            token_budget: 24000,
            api_budget: 60,
            step_tokens: 2048,
            output_tokens: 256,
            context_chars: 3000,
            aging_quanta: 3,
            demo_delay_ms: 900,
            max_retries: 1,
            memory_policy: MemoryPolicy::default(),
            tool_budget: default_tool_budget(),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub id: String,
    pub version: u32,
    pub name: String,
    pub role: String,
    pub instructions: String,
    pub model: String,
    pub priority: u8,
    pub memory_mb: u64,
    pub token_budget: u64,
    pub report_permission: Permission,
    #[serde(default)]
    pub read_permission: Permission,
    /// None preserves existing Ollama agent definitions and checkpoints.
    #[serde(default)]
    pub external: Option<ExternalAgent>,
    /// Reviewed service claims only; never instructions, credentials or authority.
    #[serde(default)]
    pub discovery: Option<AgentDiscovery>,
    /// User-reviewed routing labels; not tool permissions or verified abilities.
    #[serde(default)]
    pub capabilities: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryRequest {
    pub card_url: String,
    #[serde(default)]
    pub bearer_token_env: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDiscovery {
    pub card_url: String,
    pub name: String,
    pub description: String,
    pub agent_version: String,
    pub protocol_version: String,
    pub endpoint: String,
    pub streaming: bool,
    pub authentication_required: bool,
    pub skills: Vec<DiscoveredSkill>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredSkill {
    pub id: String,
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    pub input_modes: Vec<String>,
    pub output_modes: Vec<String>,
    pub authentication_required: bool,
}
impl AgentDiscovery {
    pub fn validate(&self) -> Result<(), String> {
        for endpoint in [&self.card_url, &self.endpoint] {
            ExternalAgent {
                protocol: ExternalProtocol::A2aJsonRpc,
                endpoint: endpoint.clone(),
                timeout_seconds: 15,
                bearer_token_env: None,
                send_context: false,
            }
            .validate()?;
        }
        let valid = |s: &str, max| {
            !s.trim().is_empty()
                && s.len() <= max
                && !s.chars().any(|c| c.is_control() && c != '\n' && c != '\t')
        };
        if self.protocol_version != "0.3.0"
            || !valid(&self.name, 80)
            || !valid(&self.description, 2000)
            || !valid(&self.agent_version, 80)
            || self.skills.is_empty()
            || self.skills.len() > 32
        {
            return Err("Agent Card metadata exceeds supported limits or protocol version.".into());
        }
        let mut ids = std::collections::HashSet::new();
        for skill in &self.skills {
            if !valid(&skill.id, 80)
                || !valid(&skill.name, 120)
                || !valid(&skill.description, 1000)
                || !ids.insert(&skill.id)
                || skill.tags.len() > 16
                || skill.tags.iter().any(|t| !valid(t, 64))
                || [&skill.input_modes, &skill.output_modes]
                    .iter()
                    .any(|m| m.is_empty() || m.len() > 16 || m.iter().any(|s| !valid(s, 100)))
            {
                return Err("Invalid or oversized Agent Card skills.".into());
            }
        }
        if !self.skills.iter().any(|s| {
            s.input_modes.iter().any(|m| m == "text/plain")
                && s.output_modes.iter().any(|m| m == "text/plain")
        }) {
            return Err("Agent Card has no supported text/plain skill.".into());
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExternalProtocol {
    HttpJson,
    A2aJsonRpc,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalAgent {
    pub protocol: ExternalProtocol,
    /// Full execution URL, not a chat link, model API or agent-card URL.
    pub endpoint: String,
    pub timeout_seconds: u64,
    /// Credential reference only; never serialize the credential itself.
    #[serde(default)]
    pub bearer_token_env: Option<String>,
    /// Explicit consent to send the bounded task context to this service.
    #[serde(default)]
    pub send_context: bool,
}
impl ExternalAgent {
    pub fn validate(&self) -> Result<(), String> {
        let url = url::Url::parse(&self.endpoint)
            .map_err(|_| "External agent needs a valid execution URL.")?;
        let loopback = match url.host() {
            Some(url::Host::Domain(name)) => name == "localhost",
            Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
            Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
            None => false,
        };
        if self.endpoint.len() > 2048
            || url.host().is_none()
            || !(url.scheme() == "https" || url.scheme() == "http" && loopback)
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || !(1..=120).contains(&self.timeout_seconds)
        {
            return Err("Use HTTPS (or HTTP on loopback), no URL credentials/query/fragment, and a timeout of 1–120 seconds.".into());
        }
        if let Some(name) = &self.bearer_token_env {
            if !name.starts_with("ORCHESTRATION_AGENT_")
                || name.len() <= "ORCHESTRATION_AGENT_".len()
                || name.len() > 100
                || !name
                    .bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
            {
                return Err("Token variable must use ORCHESTRATION_AGENT_ followed by uppercase letters, digits or underscores.".into());
            }
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDefinition {
    pub id: String,
    pub name: String,
    pub agent_id: String,
    #[serde(default)]
    pub assignment: TaskAssignment,
    #[serde(default)]
    pub required_capabilities: Vec<String>,
    pub dependencies: Vec<String>,
    pub steps: u32,
    pub publish_report: bool,
    pub fail_once: bool,
    /// Earliest eligibility relative to run start. Zero preserves older templates.
    #[serde(default)]
    pub release_after_ms: u64,
    /// Demo-only exclusive logical resources, acquired one per step and retained
    /// until model work ends. These are not OS locks or tool permissions.
    #[serde(default)]
    pub resource_plan: Vec<String>,
    #[serde(default)]
    pub document_id: Option<String>,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskAssignment {
    #[default]
    Manual,
    Capabilities,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssignmentDecision {
    pub eligible_agent_ids: Vec<String>,
    pub reason: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workflow {
    pub id: String,
    pub name: String,
    pub tasks: Vec<TaskDefinition>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub sequence: u64,
    pub at_ms: u64,
    pub kind: String,
    pub task_id: Option<String>,
    pub message: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRun {
    pub definition: TaskDefinition,
    pub agent: Agent,
    #[serde(default)]
    pub assignment_decision: Option<AssignmentDecision>,
    pub status: TaskStatus,
    pub completed_steps: u32,
    pub attempts: u32,
    #[serde(default)]
    pub model_attempts: u32,
    pub retries: u32,
    pub ready_since: u64,
    pub last_served: u64,
    pub tokens: u64,
    pub reserved_tokens: u64,
    pub uncertain_tokens: u64,
    pub attempt_id: Option<String>,
    pub approval: Option<bool>,
    pub output: String,
    pub wait_reason: String,
    #[serde(default)]
    pub wait: Option<WaitKind>,
    #[serde(default)]
    pub document: Option<DocumentAccess>,
    #[serde(default)]
    pub remote: Option<RemoteTask>,
}
/// The latest A2A step, pinned to the task's saved agent endpoint/version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteTask {
    pub submission_id: String,
    pub task_id: Option<String>,
    pub step: u32,
    pub state: String,
    pub pending: bool,
    pub updated_at: u64,
    /// Portion of task.uncertain_tokens belonging to this unresolved step.
    pub uncertain_tokens: u64,
    pub cancellation: RemoteCancellation,
    pub detail: String,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RemoteCancellation {
    None,
    Requested,
    Sent,
    Confirmed,
    Unconfirmed,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WaitKind {
    Arrival,
    Dependency,
    Worker,
    Agent,
    Memory,
    HostMemory,
    Tokens,
    Resource { resource: String, owner: String },
    Approval,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResourceEdge {
    pub waiter: String,
    pub resource: String,
    pub owner: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeadlockWitness {
    pub resource_version: u64,
    pub edges: Vec<ResourceEdge>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeadlockRecovery {
    pub at_ms: u64,
    pub witness: DeadlockWitness,
    pub victim: String,
    pub released: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub id: String,
    pub task_id: String,
    pub name: String,
    pub text: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Run {
    pub id: String,
    pub name: String,
    pub prompt: String,
    pub status: RunStatus,
    pub settings: Settings,
    pub started_at: u64,
    pub finished_at: Option<u64>,
    pub tasks: Vec<TaskRun>,
    pub events: Vec<Event>,
    pub artifacts: Vec<Artifact>,
    pub clock: u64,
    pub event_sequence: u64,
    pub api_calls: u64,
    #[serde(default)]
    pub resource_owners: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub resource_version: u64,
    #[serde(default)]
    pub deadlock_recoveries: Vec<DeadlockRecovery>,
    #[serde(default)]
    pub memory: Vec<MemoryEntry>,
    #[serde(default)]
    pub memory_version: u32,
    #[serde(default)]
    pub tool_calls: u64,
}
impl Run {
    pub fn tokens(&self) -> u64 {
        self.tasks.iter().map(|t| t.tokens).sum()
    }
    pub fn reserved(&self) -> u64 {
        self.tasks.iter().map(|t| t.reserved_tokens).sum()
    }
    pub fn uncertain(&self) -> u64 {
        self.tasks.iter().map(|t| t.uncertain_tokens).sum()
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub schema_version: u32,
    pub settings: Settings,
    pub agents: Vec<Agent>,
    pub workflows: Vec<Workflow>,
    pub runs: Vec<Run>,
    #[serde(default)]
    pub documents: Vec<Document>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobKind {
    Model,
    ReadDocument(DocumentRequest),
}
#[derive(Debug, Clone)]
pub struct Job {
    pub kind: JobKind,
    pub run_id: String,
    pub task_id: String,
    pub attempt_id: String,
    pub agent: Agent,
    pub prompt: String,
    pub context: String,
    pub step: u32,
    pub settings: Settings,
    pub fail_demo: bool,
}
#[derive(Debug, Clone)]
pub struct StepResult {
    pub text: String,
    pub tokens: Option<u64>,
    pub error: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostMetrics {
    pub cpu_percent: f32,
    pub used_memory_mb: u64,
    pub total_memory_mb: u64,
    pub available_memory_mb: u64,
    pub gpu: Option<f32>,
}
