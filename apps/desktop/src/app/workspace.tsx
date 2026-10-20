import { createContext, useCallback, useContext, useEffect, useRef, useState, type ReactNode } from 'react';
import { invoke, isTauri } from '@tauri-apps/api/core';
import type { Snapshot, Workflow } from '../types';

/** Commands accepted by the Rust `command` handler that the React pages send. */
export type Command =
  | { type: 'launch'; workflow_id: string; prompt: string }
  | { type: 'control'; run_id: string; action: 'pause' | 'resume' | 'stop' }
  | { type: 'approve'; run_id: string; task_id: string; allow: boolean }
  | { type: 'approve_document'; run_id: string; task_id: string; request_id: string; allow: boolean }
  | { type: 'save_workflow'; workflow: Workflow };

interface WorkspaceValue {
  /** False in a plain browser preview: there is no Rust engine to talk to. */
  connected: boolean;
  snapshot?: Snapshot;
  lastSync?: Date;
  busy: boolean;
  send: (command: Command) => Promise<boolean>;
  toast: (message: string, error?: boolean) => void;
}

const WorkspaceContext = createContext<WorkspaceValue | null>(null);
const POLL_MS = 800;

export function WorkspaceProvider({ children }: { children: ReactNode }) {
  const connected = isTauri();
  const [snapshot, setSnapshot] = useState<Snapshot>();
  const [lastSync, setLastSync] = useState<Date>();
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<{ text: string; error: boolean } | null>(null);
  const timer = useRef<number | undefined>(undefined);

  const toast = useCallback((text: string, error = false) => {
    setMessage({ text, error });
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => setMessage(null), 6500);
  }, []);

  const refresh = useCallback(async () => {
    try {
      setSnapshot(await invoke<Snapshot>('snapshot'));
      setLastSync(new Date());
    } catch (e) {
      toast(String(e), true);
    }
  }, [toast]);

  useEffect(() => {
    if (!connected) return;
    void refresh();
    const id = window.setInterval(() => void refresh(), POLL_MS);
    return () => window.clearInterval(id);
  }, [connected, refresh]);

  const send = useCallback(async (command: Command) => {
    if (!connected) { toast('Open the desktop app to run this against the engine.', true); return false; }
    setBusy(true);
    try {
      await invoke('command', { command });
      await refresh();
      return true;
    } catch (e) {
      toast(String(e), true);
      return false;
    } finally {
      setBusy(false);
    }
  }, [connected, refresh, toast]);

  return (
    <WorkspaceContext.Provider value={{ connected, snapshot, lastSync, busy, send, toast }}>
      {children}
      {/* Mounted only while showing: the classic pages keep their own role=status toast. */}
      {message && <div className={`mx-toast visible${message.error ? ' error' : ''}`} role="status" aria-live="polite">{message.text}</div>}
    </WorkspaceContext.Provider>
  );
}

export function useWorkspace() {
  const value = useContext(WorkspaceContext);
  if (!value) throw new Error('useWorkspace must be used inside WorkspaceProvider');
  return value;
}
