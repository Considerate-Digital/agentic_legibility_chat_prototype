export interface ProviderConfig {
  base_url: string
  api_key: string
  model: string
}

export interface AnalyserConfig {
  model: string
  base_url?: string
  api_key?: string
}

export interface AppConfig {
  provider: ProviderConfig
  analyser?: AnalyserConfig
  states_override_dir?: string
  tools_override_dir?: string
  cards_override_dir?: string
  live_resources_dir?: string
  cards_enabled: boolean
}

export interface LiveResourcesSummary {
  path: string
  endpoints: number
  services: number
  plans: number
  is_valid: boolean
}

export interface SetupStatus {
  has_provider: boolean
  has_analyser: boolean
  has_live_resources_dir: boolean
  spec_tools_ready: boolean
}

export interface StateView {
  name: string
  description: string
  valid_transitions: string[]
  tools: string[]
  system_prompt: string
}

export interface StateSummary {
  name: string
  description: string
}

export interface PlaygroundFiles {
  states: string[]
  tools: string[]
  cards: string[]
}

export interface CardView {
  name: string
  content: string
  css?: string
}

export interface ServiceStepEvent {
  service_id: string
  service_name: string
  step_number: number
  total_steps: number
  endpoint_id: string
  endpoint_name: string
  department: string
  required: boolean
  status: 'starting' | 'completed' | 'skipped' | 'failed'
}

export interface UiInputRequest {
  input_type: 'text' | 'number' | 'date' | 'email' | 'select'
  name: string
  description: string
  options?: string[]
}

export type MessageRole = 'user' | 'assistant' | 'tool-call' | 'card' | 'error'

export interface Message {
  id: string
  role: MessageRole
  content: string
  toolName?: string
  /** Which MCP server answered (only "state" exists now). */
  toolServer?: string
  toolArgs?: unknown
  toolResult?: string
}
