
import React, { useRef, useEffect, useState, useCallback } from 'react';
import { XMarkIcon } from './icons';

function buildCssSelector(element: Element, root: Element): string {
  const path: string[] = [];
  let current: Element | null = element;

  while (current && current !== root) {
    const parent = current.parentElement;
    const tagName = current.tagName.toLowerCase();

    let segment = tagName;

    const id = current.getAttribute('id');
    if (id) {
      segment = `#${CSS.escape(id)}`;
      path.unshift(segment);
      break;
    }

    const classes = Array.from(current.classList).filter(c => c && !/^(css|styled|sc)-[a-zA-Z0-9]{6,}$/.test(c));
    if (classes.length > 0) {
      segment += '.' + classes.map(c => CSS.escape(c)).join('.');
    }

    if (parent) {
      const siblings = Array.from(parent.children).filter(s => s.tagName === current!.tagName);
      if (siblings.length > 1) {
        const index = siblings.indexOf(current) + 1;
        segment += `:nth-of-type(${index})`;
      }
    }

    path.unshift(segment);
    current = parent;
  }

  return path.join(' > ');
}

interface InspectorPanelProps {
  htmlContent: string;
  baseUrl?: string;
  isPicking: boolean;
  onClose: () => void;
  onSelectorPicked: (selector: string) => void;
  highlightedSelector: string | null;
}

const InspectorPanel: React.FC<InspectorPanelProps> = ({ htmlContent, baseUrl, isPicking, onClose, onSelectorPicked, highlightedSelector }) => {
  const iframeRef = useRef<HTMLIFrameElement>(null);
  const [iframeBody, setIframeBody] = useState<HTMLBodyElement | null>(null);
  const [toast, setToast] = useState<{ visible: boolean; x: number; y: number; message: string } | null>(null);
  const highlightedElementsRef = useRef<HTMLElement[]>([]);
  const pickedRef = useRef(false);

  const wrappedHtml = React.useMemo(() => {
    if (!htmlContent) return '';

    // Safely strip all script tags to protect Tauri shell from remote code execution since sandbox is omitted
    const sanitizedHtml = htmlContent.replace(/<script\b[^<]*(?:(?!<\/script>)<[^<]*)*<\/script>/gi, '');

    // If it's already a full HTML document, use as-is
    if (sanitizedHtml.trim().toLowerCase().startsWith('<!doctype html') || sanitizedHtml.includes('<html')) {
      // Inject base tag if baseUrl is provided
      if (baseUrl) {
        const baseTag = `<base href="${baseUrl}">`;
        if (sanitizedHtml.includes('<head>')) {
          return sanitizedHtml.replace('<head>', `<head>${baseTag}`);
        } else if (sanitizedHtml.includes('<HEAD>')) {
          return sanitizedHtml.replace('<HEAD>', `<HEAD>${baseTag}`);
        } else {
          // No head found, wrap it
          return `<!DOCTYPE html><html><head>${baseTag}</head><body>${sanitizedHtml}</body></html>`;
        }
      }
      return sanitizedHtml;
    }

    // Raw HTML fragment - wrap in a full document
    const baseTag = baseUrl ? `<base href="${baseUrl}">` : '';
    return `<!DOCTYPE html>
<html>
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  ${baseTag}
  <style>
    /* Ensure inspector can interact with elements */
    * { cursor: inherit; }
    img { max-width: 100%; height: auto; }
  </style>
</head>
<body>
${sanitizedHtml}
</body>
</html>`;
  }, [htmlContent, baseUrl]);

  // Called when the iframe finishes loading its srcDoc content.
  const handleLoad = useCallback(() => {
    const iframe = iframeRef.current;
    if (!iframe) return;
    try {
      if (iframe.contentDocument) {
        if (baseUrl) {
          const existingBase = iframe.contentDocument.querySelector('base');
          if (!existingBase) {
            const base = iframe.contentDocument.createElement('base');
            base.href = baseUrl;
            iframe.contentDocument.head.appendChild(base);
          }
        }
        setIframeBody(iframe.contentDocument.body as HTMLBodyElement);
      }
    } catch (err) {
      console.error('Error accessing iframe contentDocument:', err);
    }
  }, [baseUrl]);

  // Effect to highlight elements based on the `highlightedSelector` prop.
  useEffect(() => {
    // Cleanup previous highlights first
    highlightedElementsRef.current.forEach(el => {
      el.style.outline = '';
      el.style.outlineOffset = '';
      el.style.boxShadow = '';
    });
    highlightedElementsRef.current = [];

    if (!iframeBody || !highlightedSelector) {
      return;
    }

    try {
      const elements = iframeBody.querySelectorAll(highlightedSelector);
      if (elements.length > 0) {
        elements.forEach(el => {
          const htmlEl = el as HTMLElement;
          htmlEl.style.outline = '3px dashed #ea580c'; // orange-600
          htmlEl.style.outlineOffset = '2px';
          htmlEl.style.boxShadow = '0 0 10px 2px rgba(234, 88, 12, 0.6)';
          highlightedElementsRef.current.push(htmlEl);
        });

        // Scroll the first element into view
        (elements[0] as HTMLElement).scrollIntoView({ behavior: 'smooth', block: 'center' });
      }
    } catch (e) {
      console.error(`Invalid selector for highlighting: "${highlightedSelector}"`, e);
    }

    // Cleanup function for this effect
    return () => {
      highlightedElementsRef.current.forEach(el => {
        el.style.outline = '';
        el.style.outlineOffset = '';
        el.style.boxShadow = '';
      });
      highlightedElementsRef.current = [];
    };
  }, [iframeBody, highlightedSelector]);


  // Effect to manage all event listeners within the iframe.
  // It runs whenever the iframe body is ready or the picking state changes.
  useEffect(() => {
    if (!iframeBody) return;

    const iframeDoc = iframeBody.ownerDocument;
    let lastHovered: HTMLElement | null = null;

    // --- Handler for highlighting elements on mouseover ---
    const handleMouseover = (e: MouseEvent) => {
      const target = e.target as HTMLElement;
      if (target && target !== lastHovered) {
        if (lastHovered) {
          lastHovered.style.outline = '';
        }
        target.style.outline = '2px solid #3b82f6';
        lastHovered = target;
      }
    };

    // --- Handler for removing highlight ---
    const handleMouseout = (e: MouseEvent) => {
      const target = e.target as HTMLElement;
      if (target) {
        target.style.outline = '';
        lastHovered = null;
      }
    };

    // --- Handler for left-clicking to select an element ---
    const handleClick = (e: MouseEvent) => {
      e.preventDefault();
      e.stopPropagation();
      const target = e.target as HTMLElement;
      if (target && target !== iframeBody && target !== iframeDoc.documentElement) {
        target.style.outline = '';
        const selector = buildCssSelector(target, iframeBody);
        pickedRef.current = true;
        onSelectorPicked(selector);
      }
    };

    // --- Handler for right-clicking to copy selector ---
    const handleContextMenu = (e: MouseEvent) => {
      e.preventDefault();
      const target = e.target as HTMLElement;
      if (target && target !== iframeBody && target !== iframeDoc.documentElement) {
        const selector = buildCssSelector(target, iframeBody);
        navigator.clipboard.writeText(selector).then(() => {
          setToast({
            visible: true,
            x: e.clientX,
            y: e.clientY,
            message: 'Selector Copied!'
          });
          setTimeout(() => setToast(null), 2000);
        }).catch(err => {
          console.error("Failed to copy selector: ", err);
          setToast({
            visible: true,
            x: e.clientX,
            y: e.clientY,
            message: 'Copy failed!'
          });
          setTimeout(() => setToast(null), 2000);
        });
      }
    };

    // Attach listeners to the iframe's document (not just body) to catch all clicks
    iframeDoc.addEventListener('contextmenu', handleContextMenu);

    if (isPicking) {
      iframeDoc.addEventListener('mouseover', handleMouseover);
      iframeDoc.addEventListener('mouseout', handleMouseout);
      iframeDoc.addEventListener('click', handleClick, true);
    }

    // Cleanup function to remove listeners
    return () => {
      if (lastHovered) lastHovered.style.outline = '';
      iframeDoc.removeEventListener('contextmenu', handleContextMenu);
      if (isPicking) {
        iframeDoc.removeEventListener('mouseover', handleMouseover);
        iframeDoc.removeEventListener('mouseout', handleMouseout);
        iframeDoc.removeEventListener('click', handleClick, true);
      }
    };
  }, [iframeBody, isPicking, onSelectorPicked]);

  // Effect for toast visibility transition
  useEffect(() => {
    if (toast?.visible) {
      const timer = setTimeout(() => {
        setToast(t => (t ? { ...t, visible: false } : null));
      }, 1800); // Start fade out before removing
      return () => clearTimeout(timer);
    }
  }, [toast?.visible]);


  return (
    <div className="h-1/2 flex flex-col border-t-2 border-gray-300 bg-white shadow-lg">
      <header className="flex justify-between items-center p-2 bg-gray-100 border-b">
        <h3 className="font-bold text-gray-800">Inspector Preview</h3>
        <div className="flex items-center gap-4">
          <span className="text-sm text-gray-600 hidden md:block">Right-click to copy selector</span>
          {isPicking && (
            <div className="p-1 px-3 bg-blue-100 text-blue-800 rounded-full text-sm font-semibold animate-pulse">
              Picking Element...
            </div>
          )}
        </div>
        <button
          onClick={onClose}
          className="p-2 text-gray-600 rounded-md hover:bg-gray-200 hover:text-gray-900"
          title="Close Inspector"
        >
          <XMarkIcon />
        </button>
      </header>
      <div className="flex-1 overflow-hidden relative">
        <iframe
          ref={iframeRef}
          srcDoc={wrappedHtml}
          title="Inspector Preview"
          onLoad={handleLoad}
          className={`w-full h-full border-0 ${isPicking ? 'cursor-crosshair' : 'cursor-default'}`}
        />
        {toast && (
          <div
            className={`absolute p-2 bg-black text-white text-sm rounded-md shadow-lg transition-opacity duration-200 pointer-events-none ${toast.visible ? 'opacity-75' : 'opacity-0'}`}
            style={{ left: `${toast.x + 15}px`, top: `${toast.y + 10}px`, zIndex: 100 }}
          >
            {toast.message}
          </div>
        )}
      </div>
    </div>
  );
};

export default InspectorPanel;