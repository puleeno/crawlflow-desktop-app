import React from 'react';
import ReactDOM from 'react-dom/client';
import './index.css';
import App from './App';
import { pluginManager } from './lib/pluginManager';
import { builtinPlugins } from './lib/plugins/builtin';

const rootElement = document.getElementById('root');
if (!rootElement) {
  throw new Error("Could not find root element to mount to");
}

// Suppress the benign "ResizeObserver loop" error.
const resizeObserverLoopErr = /ResizeObserver loop limit exceeded|ResizeObserver loop completed with undelivered notifications/;

const originalError = console.error;
console.error = (...args) => {
  if (args.length > 0 && typeof args[0] === 'string' && resizeObserverLoopErr.test(args[0])) {
    return;
  }
  originalError.call(console, ...args);
};

window.addEventListener('error', (event) => {
  if (typeof event.message === 'string' && resizeObserverLoopErr.test(event.message)) {
    event.stopImmediatePropagation();
  }
});

// Initialize plugin system
builtinPlugins.forEach(p => pluginManager.register(p));
pluginManager.init().catch(console.error);

const root = ReactDOM.createRoot(rootElement);
root.render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
