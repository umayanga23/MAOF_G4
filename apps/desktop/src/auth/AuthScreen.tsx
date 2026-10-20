import { useState, type FormEvent } from 'react';
import nodeGraph from '../assets/node-graph.svg';
import googleG from '../assets/google-g.svg';
import spinner from '../assets/spinner.svg';
import './auth.css';

type Tab = 'login' | 'signup';
/** 'waiting' and 'signed-in' are designed Google states; they render once an account backend exists. */
export type AuthStatus = { kind: 'form' } | { kind: 'waiting' } | { kind: 'signed-in'; email: string; provider: string };

// The Rust engine has no account system yet, so every account action explains that and points to offline mode.
const ACCOUNTS_UNAVAILABLE = {
  title: 'Accounts are not available yet.',
  detail: 'Use “Continue offline” to work locally on this PC.',
};

function strength(password: string) {
  return [password.length >= 8, /\d/.test(password), /[A-Z]/.test(password), password.length >= 12 && /[^A-Za-z0-9]/.test(password)]
    .filter(Boolean).length;
}

function BrandPanel() {
  return (
    <div className="auth-brand">
      <div className="auth-logo">
        <div className="auth-logo-mark" />
        <span className="auth-logo-text">MAOF</span>
      </div>
      <img className="auth-graph" src={nodeGraph} width={420} height={260.75} alt="" />
      <p className="auth-tagline">Run your AI agents with control.</p>
      <p className="auth-version">v0.1.0</p>
    </div>
  );
}

function PasswordInput({ placeholder, value, onChange, autoComplete }: { placeholder: string; value: string; onChange: (v: string) => void; autoComplete: string }) {
  const [visible, setVisible] = useState(false);
  return (
    <div className="auth-input">
      <input name="password" type={visible ? 'text' : 'password'} placeholder={placeholder} autoComplete={autoComplete} value={value} onChange={e => onChange(e.target.value)} />
      <button type="button" className="auth-show" onClick={() => setVisible(v => !v)} aria-pressed={visible}>{visible ? 'Hide' : 'Show'}</button>
    </div>
  );
}

export function AuthScreen({ onContinueOffline, status = { kind: 'form' } }: { onContinueOffline: () => void; status?: AuthStatus }) {
  const [tab, setTab] = useState<Tab>('login');
  const [notice, setNotice] = useState<typeof ACCOUNTS_UNAVAILABLE | null>(null);
  const [name, setName] = useState('');
  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const [keepSignedIn, setKeepSignedIn] = useState(true);
  const [agreed, setAgreed] = useState(false);
  const signup = tab === 'signup';
  const unavailable = () => setNotice(ACCOUNTS_UNAVAILABLE);
  const submit = (e: FormEvent) => { e.preventDefault(); unavailable(); };
  const switchTab = (next: Tab) => { setTab(next); setNotice(null); setPassword(''); };
  const filled = strength(password);

  return (
    <div className="auth">
      <BrandPanel />
      <main className="auth-panel">
        {status.kind === 'waiting' ? (
          <div className="auth-card"><div className="auth-status" role="status">
            <img className="auth-spinner" src={spinner} width={36} height={36} alt="" />
            <h2>Continue in your browser</h2>
            <p className="auth-sub">We opened Google sign-in in your default browser. Pick your Gmail account there, then come back to this window.</p>
            <div className="auth-actions">
              <button type="button" className="auth-btn auth-btn-outline compact">Cancel</button>
              <button type="button" className="auth-btn auth-btn-ghost">Open again</button>
            </div>
          </div></div>
        ) : status.kind === 'signed-in' ? (
          <div className="auth-card"><div className="auth-status">
            <div className="auth-done" aria-hidden="true">✓</div>
            <h2>You're signed in</h2>
            <p className="auth-sub">{status.email}</p>
            <p className="auth-sub">Connected with {status.provider}</p>
            <button type="button" className="auth-btn auth-btn-primary" onClick={onContinueOffline}>Open dashboard</button>
            <button type="button" className="auth-btn auth-btn-ghost">Switch account</button>
          </div></div>
        ) : (
          <div className="auth-card">
            <div className="auth-title">
              <h1>{signup ? 'Create your account' : 'Welcome back'}</h1>
              <p className="auth-sub">{signup ? 'Start running agents in a few minutes.' : 'Log in to manage your agents.'}</p>
            </div>
            <div className="auth-tabs" role="tablist">
              <button type="button" role="tab" className="auth-tab" aria-selected={!signup} onClick={() => switchTab('login')}>Log in</button>
              <button type="button" role="tab" className="auth-tab" aria-selected={signup} onClick={() => switchTab('signup')}>Sign up</button>
            </div>
            {notice && (
              <div className="auth-error" role="alert">
                <strong>{notice.title}</strong>
                <span>{notice.detail}</span>
              </div>
            )}
            <button type="button" className="auth-btn auth-btn-outline" onClick={unavailable}>
              <img src={googleG} width={20} height={20} alt="" />
              {signup ? 'Sign up with Google' : 'Continue with Google'}
            </button>
            <div className="auth-divider">or use email</div>
            <form className="auth-form" onSubmit={submit} noValidate>
              {signup && (
                <label className="auth-field">Full name
                  <div className="auth-input"><input name="name" autoComplete="name" placeholder="Your name" value={name} onChange={e => setName(e.target.value)} /></div>
                </label>
              )}
              <label className="auth-field">Email
                <div className="auth-input"><input name="email" type="email" autoComplete="email" placeholder="you@gmail.com" value={email} onChange={e => setEmail(e.target.value)} /></div>
              </label>
              <label className="auth-field">Password
                <PasswordInput placeholder={signup ? 'At least 8 characters' : 'Your password'} autoComplete={signup ? 'new-password' : 'current-password'} value={password} onChange={setPassword} />
              </label>
              {signup ? (
                <>
                  <div className="auth-strength" aria-hidden="true">{[0, 1, 2, 3].map(i => <span key={i} className={i < filled ? 'on' : ''} />)}</div>
                  <p className="auth-hint">Use 8+ characters with a number and a capital letter.</p>
                  <label className="auth-check"><input type="checkbox" checked={agreed} onChange={e => setAgreed(e.target.checked)} />I agree to the Terms and Privacy Policy</label>
                  <button type="submit" className="auth-btn auth-btn-primary">Create account</button>
                </>
              ) : (
                <>
                  <div className="auth-options">
                    <label className="auth-check"><input type="checkbox" checked={keepSignedIn} onChange={e => setKeepSignedIn(e.target.checked)} />Keep me signed in</label>
                    <button type="button" className="auth-link" onClick={unavailable}>Forgot password?</button>
                  </div>
                  <button type="submit" className="auth-btn auth-btn-primary">Log in</button>
                </>
              )}
            </form>
            <div className="auth-offline">
              <div className="auth-offline-pc" aria-hidden="true" />
              <div className="auth-offline-text">
                <strong>No account? Work offline.</strong>
                <span>Local mode keeps everything on this PC.</span>
                <button type="button" className="auth-link" onClick={onContinueOffline}>Continue offline</button>
              </div>
            </div>
          </div>
        )}
      </main>
    </div>
  );
}
