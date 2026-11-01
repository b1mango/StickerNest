import React from 'react';
import ReactDOM from 'react-dom/client';
import './styles.css';

// `npm run dev` + `?mock` runs the UI against an in-browser fake backend.
const useMock = import.meta.env.DEV && new URLSearchParams(location.search).has('mock');

const boot = useMock
  ? (Object.assign(globalThis, { isTauri: true }), import('./dev/mock'))
      .then(async mock => (await import('./api')).setAssetUrlOverride(mock.assetUrl))
  : Promise.resolve();

void boot.then(() => import('./App')).then(({ default: App }) => {
  ReactDOM.createRoot(document.getElementById('root')!).render(
    <React.StrictMode><App /></React.StrictMode>,
  );
});
