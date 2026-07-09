import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { RawItem, ItemsSummary } from '../types';

const STATUS_TABS = ['all', 'pending', 'processing', 'done', 'error', 'ignored', 'crawled'] as const;
type StatusTab = (typeof STATUS_TABS)[number];

const STATUS_COLORS: Record<string, string> = {
  pending: '#f59e0b',
  processing: '#3b82f6',
  done: '#22c55e',
  error: '#ef4444',
  ignored: '#6b7280',
  crawled: '#06b6d4',
};

interface Props {
  projectId: string;
  onClose: () => void;
}

export function RawItemsBrowser({ projectId, onClose }: Props) {
  const [items, setItems] = useState<RawItem[]>([]);
  const [summary, setSummary] = useState<ItemsSummary | null>(null);
  const [activeTab, setActiveTab] = useState<StatusTab>('all');
  const [search, setSearch] = useState('');
  const [loading, setLoading] = useState(true);
  const [expandedId, setExpandedId] = useState<number | null>(null);
  const [page, setPage] = useState(0);
  const [total, setTotal] = useState(0);
  const pageSize = 30;

  const fetchSummary = useCallback(async () => {
    try {
      const s = await invoke<ItemsSummary>('get_raw_items_summary_cmd', { projectId });
      setSummary(s);
    } catch (e) {
      console.error('Failed to fetch summary:', e);
    }
  }, [projectId]);

  const fetchItems = useCallback(async () => {
    setLoading(true);
    try {
      const status = activeTab === 'all' ? null : activeTab;
      const result = await invoke<{ items: RawItem[]; total: number }>('get_raw_items_cmd', {
        projectId,
        status,
        search: search || null,
        limit: pageSize,
        offset: page * pageSize,
        sortBy: 'created_at',
        sortDir: 'DESC',
      });
      setItems(result.items);
      setTotal(result.total);
    } catch (e) {
      console.error('Failed to fetch items:', e);
    }
    setLoading(false);
  }, [projectId, activeTab, search, page]);

  useEffect(() => {
    fetchSummary();
  }, [fetchSummary]);

  useEffect(() => {
    setPage(0);
  }, [activeTab, search]);

  useEffect(() => {
    fetchItems();
  }, [fetchItems]);

  const totalPages = Math.max(1, Math.ceil(total / pageSize));

  return (
    <div style={{
      position: 'fixed', inset: 0, zIndex: 100,
      background: 'rgba(0,0,0,0.5)', display: 'flex',
      alignItems: 'center', justifyContent: 'center',
    }}>
      <div style={{
        background: '#1a1a2e', borderRadius: 12, width: '90%', maxWidth: 1200,
        maxHeight: '85vh', display: 'flex', flexDirection: 'column',
        boxShadow: '0 20px 60px rgba(0,0,0,0.5)',
      }}>
        {/* Header */}
        <div style={{
          display: 'flex', alignItems: 'center', justifyContent: 'space-between',
          padding: '16px 24px', borderBottom: '1px solid #2d2d4a',
        }}>
          <h2 style={{ margin: 0, fontSize: 18, fontWeight: 600, color: '#e2e8f0' }}>
            Raw Items Browser
          </h2>
          <button onClick={onClose} style={{
            background: 'none', border: 'none', color: '#94a3b8', cursor: 'pointer',
            fontSize: 20, padding: '4px 8px', borderRadius: 4,
          }}>✕</button>
        </div>

        {/* Summary bar */}
        <div style={{
          display: 'flex', gap: 16, padding: '12px 24px',
          borderBottom: '1px solid #2d2d4a', flexWrap: 'wrap',
        }}>
          {summary && STATUS_TABS.map(tab => {
            const count = tab === 'all' ? summary.total : summary[tab as keyof ItemsSummary] as number;
            return (
              <div key={tab} onClick={() => setActiveTab(tab)} style={{
                display: 'flex', alignItems: 'center', gap: 6, padding: '6px 14px',
                borderRadius: 8, cursor: 'pointer', fontSize: 13,
                background: activeTab === tab ? 'rgba(59,130,246,0.15)' : 'transparent',
                border: activeTab === tab ? '1px solid rgba(59,130,246,0.3)' : '1px solid transparent',
                color: activeTab === tab ? '#60a5fa' : '#94a3b8',
              }}>
                <span style={{
                  width: 8, height: 8, borderRadius: '50%',
                  background: tab === 'all' ? '#60a5fa' : (STATUS_COLORS[tab] || '#6b7280'),
                  display: 'inline-block',
                }} />
                <span style={{ textTransform: 'capitalize' }}>{tab}</span>
                <span style={{ fontWeight: 600, marginLeft: 4 }}>{count}</span>
              </div>
            );
          })}
        </div>

        {/* Search bar */}
        <div style={{ padding: '12px 24px', borderBottom: '1px solid #2d2d4a' }}>
          <input
            placeholder="Search by URL or content..."
            value={search}
            onChange={e => setSearch(e.target.value)}
            style={{
              width: '100%', padding: '8px 12px', borderRadius: 6, border: '1px solid #3b3b5c',
              background: '#16162a', color: '#e2e8f0', fontSize: 13, outline: 'none',
              boxSizing: 'border-box',
            }}
          />
        </div>

        {/* Table */}
        <div style={{ flex: 1, overflow: 'auto', padding: 0 }}>
          {loading ? (
            <div style={{ textAlign: 'center', padding: 40, color: '#64748b' }}>Loading...</div>
          ) : items.length === 0 ? (
            <div style={{ textAlign: 'center', padding: 40, color: '#64748b' }}>No items found</div>
          ) : (
            <table style={{ width: '100%', borderCollapse: 'collapse', fontSize: 13 }}>
              <thead>
                <tr style={{ borderBottom: '1px solid #2d2d4a', color: '#94a3b8' }}>
                  <th style={{ padding: '10px 16px', textAlign: 'left', fontWeight: 500 }}>ID</th>
                  <th style={{ padding: '10px 16px', textAlign: 'left', fontWeight: 500 }}>URL</th>
                  <th style={{ padding: '10px 16px', textAlign: 'left', fontWeight: 500 }}>Type</th>
                  <th style={{ padding: '10px 16px', textAlign: 'left', fontWeight: 500 }}>Status</th>
                  <th style={{ padding: '10px 16px', textAlign: 'left', fontWeight: 500 }}>Matched</th>
                  <th style={{ padding: '10px 16px', textAlign: 'left', fontWeight: 500 }}>Worker</th>
                  <th style={{ padding: '10px 16px', textAlign: 'left', fontWeight: 500 }}>Dups</th>
                  <th style={{ padding: '10px 16px', textAlign: 'left', fontWeight: 500 }}>Created</th>
                </tr>
              </thead>
              <tbody>
                {items.map(item => (
                  <React.Fragment key={item.id}>
                    <tr 
                      onClick={() => setExpandedId(expandedId === item.id ? null : item.id)}
                      style={{
                        borderBottom: '1px solid #252540', cursor: 'pointer',
                        background: expandedId === item.id ? 'rgba(59,130,246,0.08)' : undefined,
                      }}
                    >
                      <td style={{ padding: '10px 16px', color: '#64748b' }}>{item.id}</td>
                      <td style={{
                        padding: '10px 16px', color: '#e2e8f0', maxWidth: 350,
                        overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
                      }}>
                        {item.extracted_url || item.source_url}
                      </td>
                      <td style={{ padding: '10px 16px', color: '#94a3b8' }}>{item.item_type}</td>
                      <td style={{ padding: '10px 16px' }}>
                        <span style={{
                          display: 'inline-block', padding: '2px 8px', borderRadius: 4, fontSize: 11,
                          fontWeight: 500, color: '#fff',
                          background: STATUS_COLORS[item.status] || '#6b7280',
                        }}>
                          {item.status}
                        </span>
                      </td>
                      <td style={{ padding: '10px 16px', color: '#94a3b8' }}>
                        {item.matched === 1 ? '✓' : item.matched === -1 ? '✗' : '—'}
                      </td>
                      <td style={{ padding: '10px 16px', color: '#94a3b8', maxWidth: 120, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                        {item.worker_id || '—'}
                      </td>
                      <td style={{ padding: '10px 16px', color: '#94a3b8' }}>{item.dup_count}</td>
                      <td style={{ padding: '10px 16px', color: '#64748b', fontSize: 12 }}>
                        {item.created_at ? item.created_at.replace('T', ' ').substring(0, 19) : '—'}
                      </td>
                    </tr>
                    {expandedId === item.id && (
                      <tr>
                        <td colSpan={8} style={{ 
                          padding: '16px 24px', 
                          background: 'rgba(0,0,0,0.2)',
                          borderBottom: '1px solid #252540',
                        }}>
                          {item.raw_content ? (
                            <div>
                              <div style={{ 
                                color: '#94a3b8', 
                                fontSize: 12, 
                                marginBottom: 8,
                                fontWeight: 500,
                              }}>
                                Raw Content ({item.raw_content.length} chars)
                              </div>
                              <pre style={{
                                background: '#0f0f1a',
                                color: '#cbd5e1',
                                padding: '12px 16px',
                                borderRadius: 6,
                                fontSize: 12,
                                overflow: 'auto',
                                maxHeight: 400,
                                margin: 0,
                                whiteSpace: 'pre-wrap',
                                wordBreak: 'break-all',
                                border: '1px solid #3b3b5c',
                              }}>
                                {item.raw_content}
                              </pre>
                            </div>
                          ) : (
                            <div style={{ color: '#64748b', fontSize: 13 }}>
                              No raw content available for this item
                            </div>
                          )}
                        </td>
                      </tr>
                    )}
                  </React.Fragment>
                ))}
              </tbody>
            </table>
          )}
        </div>

        {/* Pagination */}
        <div style={{
          display: 'flex', alignItems: 'center', justifyContent: 'space-between',
          padding: '12px 24px', borderTop: '1px solid #2d2d4a',
        }}>
          <span style={{ color: '#64748b', fontSize: 13 }}>
            {total} total items
          </span>
          <div style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
            <button onClick={() => setPage(p => Math.max(0, p - 1))}
              disabled={page === 0}
              style={paginationBtnStyle(page === 0)}>
              ‹ Prev
            </button>
            <span style={{ color: '#94a3b8', fontSize: 13 }}>
              Page {page + 1} / {totalPages}
            </span>
            <button onClick={() => setPage(p => Math.min(totalPages - 1, p + 1))}
              disabled={page >= totalPages - 1}
              style={paginationBtnStyle(page >= totalPages - 1)}>
              Next ›
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

function paginationBtnStyle(disabled: boolean) {
  return {
    padding: '6px 14px', borderRadius: 6, border: '1px solid #3b3b5c',
    background: disabled ? '#1e1e38' : '#252540', color: disabled ? '#4a4a6a' : '#94a3b8',
    cursor: disabled ? 'not-allowed' : 'pointer', fontSize: 13,
  };
}
