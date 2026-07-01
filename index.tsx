
import React from 'react';
import ReactDOM from 'react-dom/client';
import App from './App';

const rootElement = document.getElementById('root');
if (!rootElement) {
  throw new Error("Could not find root element to mount to");
}

// Suppress the benign "ResizeObserver loop" error.
// This error is a browser warning that often crashes React apps in development but is safe to ignore.
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

const root = ReactDOM.createRoot(rootElement);
root.render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
