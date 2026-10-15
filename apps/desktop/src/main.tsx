import { StrictMode, useState } from 'react';
import { createRoot } from 'react-dom/client';
import './theme/tokens.css';
import { AuthScreen } from './auth/AuthScreen';
import { App } from './app/App';

// Login is a visual entry screen for now: accounts do not exist yet, so "Continue offline" opens the workspace.
function Root() {
  const [entered, setEntered] = useState(false);
  return entered ? <App /> : <AuthScreen onContinueOffline={() => setEntered(true)} />;
}

createRoot(document.getElementById('app')!).render(<StrictMode><Root /></StrictMode>);
