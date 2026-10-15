import { useEffect, useRef, useState, type ComponentType } from 'react';
import { Activity, Bell, Bot, FileText, LayoutDashboard, ListChecks, Moon, ScrollText, Search, Settings, ShieldCheck, Sun, Workflow } from 'lucide-react';
import { useWorkspace, WorkspaceProvider } from './workspace';
import { WorkflowsPage } from '../workflows/WorkflowsPage';
import './shell.css';

type NavItem = { id: string; label: string; icon: ComponentType<{ size?: number; strokeWidth?: number }> };
// ids are the classic page names: the classic UI listens for the same data-nav clicks.
const NAV: NavItem[] = [
  { id: 'workflows', label: 'Workflows', icon: Workflow },
  { id: 'overview', label: 'Dashboard', icon: LayoutDashboard },
  { id: 'agents', label: 'Agents', icon: Bot },
  { id: 'runs', label: 'Tasks', icon: ListChecks },
  { id: 'research', label: 'Monitoring', icon: Activity },
  { id: 'memory', label: 'Logs', icon: ScrollText },
  { id: 'approvals', label: 'Approvals', icon: ShieldCheck },
  { id: 'documents', label: 'Documents', icon: FileText },
];
const SETTINGS: NavItem = { id: 'settings', label: 'Settings', icon: Settings };
const THEME_KEY = 'maof.theme';

function readTheme(): 'light' | 'dark' {
  try { return localStorage.getItem(THEME_KEY) === 'dark' ? 'dark' : 'light'; } catch { return 'light'; }
}

function Shell() {
  const { connected, snapshot, lastSync } = useWorkspace();
  const [page, setPage] = useState('workflows');
  const [crumb, setCrumb] = useState('New workflow');
  const [query, setQuery] = useState('');
  const [theme, setTheme] = useState(readTheme);
  const search = useRef<HTMLInputElement>(null);

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    try { localStorage.setItem(THEME_KEY, theme); } catch { /* private window */ }
  }, [theme]);

  // The classic pages render into #legacy-root and report their own page changes (e.g. "Open run").
  useEffect(() => {
    const onPage = (e: Event) => setPage((e as CustomEvent<string>).detail);
    window.addEventListener('legacy:page', onPage);
    void import('../legacy-app');
    return () => window.removeEventListener('legacy:page', onPage);
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'k') { e.preventDefault(); search.current?.focus(); }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  const runs = snapshot?.workspace.runs ?? [];
  const activeTasks = runs.filter(r => !['completed', 'failed', 'cancelled'].includes(r.status)).flatMap(r => r.tasks);
  const running = activeTasks.filter(t => t.attempt_id).length;
  const queued = activeTasks.filter(t => ['ready', 'waiting'].includes(t.status)).length;
  const approvals = runs.flatMap(r => r.tasks).filter(t => t.status === 'approval').length;
  const badges: Record<string, number> = { runs: activeTasks.filter(t => !['completed', 'failed', 'cancelled'].includes(t.status)).length, approvals };
  const current = [...NAV, SETTINGS].find(n => n.id === page) ?? NAV[0]!;

  const navButton = (item: NavItem) => {
    const Icon = item.icon;
    const selected = item.id === page;
    return (
      <button key={item.id} type="button" className="mx-nav-item" data-nav={item.id} aria-current={selected ? 'page' : undefined} onClick={() => setPage(item.id)}>
        <Icon size={20} strokeWidth={1.5} />
        <span>{item.label}</span>
        {item.id === 'workflows' && <span className="mx-nav-home">Home</span>}
        {!!badges[item.id] && <span className="mx-nav-badge">{badges[item.id]}</span>}
      </button>
    );
  };

  return (
    <div className="mx-shell">
      <aside className="mx-sidebar">
        <div className="mx-logo"><div className="mx-logo-mark" /><span>MAOF</span></div>
        <nav className="mx-nav" aria-label="Main">{NAV.map(navButton)}</nav>
        <div className="mx-sidebar-bottom">
          <nav className="mx-nav" aria-label="Workspace">{navButton(SETTINGS)}</nav>
          <div className="mx-version">v0.1.0</div>
        </div>
      </aside>
      <div className="mx-main">
        <header className="mx-topbar">
          <span className="mx-crumb-root">{current.label}</span>
          <span className="mx-crumb-sep" aria-hidden="true">›</span>
          <span className="mx-crumb">{page === 'workflows' ? crumb : 'Workspace'}</span>
          <div className="mx-search">
            <Search size={20} strokeWidth={1.5} />
            <input ref={search} type="search" placeholder="Search agents, tasks, workflows" aria-label="Search workflows" value={query} onChange={e => setQuery(e.target.value)} />
            <kbd>Ctrl K</kbd>
          </div>
          <button type="button" className="mx-icon-btn" title="Notifications are not available yet" aria-label="Notifications"><Bell size={20} strokeWidth={1.5} /></button>
          <button type="button" className="mx-icon-btn" title={theme === 'dark' ? 'Switch to light theme' : 'Switch to dark theme'} aria-label="Toggle theme" onClick={() => setTheme(t => (t === 'dark' ? 'light' : 'dark'))}>
            {theme === 'dark' ? <Sun size={20} strokeWidth={1.5} /> : <Moon size={20} strokeWidth={1.5} />}
          </button>
          <div className="mx-avatar" title="Local workspace (offline)">ME</div>
        </header>
        <main className="mx-content">
          {page === 'workflows' && <WorkflowsPage query={query} onCrumb={setCrumb} />}
          <div id="legacy-root" className="legacy-host" data-hidden={page === 'workflows'} />
        </main>
        <footer className="mx-status">
          <span className={`mx-status-dot${connected && snapshot?.runtime.driver_running ? '' : ' off'}`} />
          <span>{!connected ? 'Browser preview · engine not connected' : snapshot?.runtime.driver_running ? 'Engine running' : 'Engine stopped'}</span>
          {connected && <span>{running} agent{running === 1 ? '' : 's'} running · {queued} task{queued === 1 ? '' : 's'} queued</span>}
          <span className="mx-status-sync">{lastSync ? `Last sync ${lastSync.toLocaleTimeString('en-GB')}` : ''}</span>
        </footer>
      </div>
    </div>
  );
}

export function App() {
  return <WorkspaceProvider><Shell /></WorkspaceProvider>;
}
