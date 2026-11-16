import type { GraphEdgeDto, GraphNodeDto, GraphDto } from './types';

export type GraphLayoutMode = 'cluster' | 'radial' | 'hierarchy';

export interface GraphConnectionSummary {
  inbound: GraphNodeDto[];
  outbound: GraphNodeDto[];
}

interface SimNode extends GraphNodeDto {
  x: number;
  y: number;
  vx: number;
  vy: number;
  radius: number;
  pinned?: boolean;
  hop?: number;
  rank?: number;
}

export interface ForceGraphOptions {
  centerPath?: string | null;
  theme?: 'light' | 'dark';
  searchQuery?: string;
  compact?: boolean;
  layoutMode?: GraphLayoutMode;
  hideOrphans?: boolean;
  folderFilter?: string;
  onNodeClick?: (node: GraphNodeDto) => void;
  onNodeSelect?: (node: GraphNodeDto | null, connections: GraphConnectionSummary) => void;
  onNodeOpen?: (node: GraphNodeDto) => void;
}

const CLUSTER_PALETTE = [
  '#2563eb',
  '#7c3aed',
  '#0891b2',
  '#d97706',
  '#059669',
  '#db2777',
  '#4f46e5',
  '#0d9488',
];

function hashString(s: string): number {
  let h = 2166136261;
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  return h >>> 0;
}

export class ForceGraphCanvas {
  private container: HTMLElement;
  private canvas: HTMLCanvasElement;
  private ctx: CanvasRenderingContext2D;
  private opts: ForceGraphOptions;
  private allNodes: SimNode[] = [];
  private allEdges: GraphEdgeDto[] = [];
  private visibleNodes: SimNode[] = [];
  private visibleEdges: GraphEdgeDto[] = [];
  private nodeById = new Map<string, SimNode>();
  private folderColors = new Map<string, string>();
  private rafId: number | null = null;
  private animTicks = 0;
  private resizeObs: ResizeObserver | null = null;

  private layoutMode: GraphLayoutMode = 'cluster';
  private hideOrphans = false;
  private folderFilter = '';

  // Camera transform
  private camX = 0;
  private camY = 0;
  private scale = 1;

  // Interaction
  private hoveredId: string | null = null;
  private selectedId: string | null = null;
  private dragNode: SimNode | null = null;
  private isPanning = false;
  private dragMoved = false;
  private downX = 0;
  private downY = 0;
  private lastMouseX = 0;
  private lastMouseY = 0;
  private lastTapTime = 0;
  private lastTapNodeId: string | null = null;

  constructor(container: HTMLElement, data: GraphDto, opts: ForceGraphOptions = {}) {
    this.container = container;
    this.opts = opts;
    this.layoutMode = opts.layoutMode || 'cluster';
    this.hideOrphans = Boolean(opts.hideOrphans);
    this.folderFilter = opts.folderFilter || '';

    this.container.innerHTML = '';
    this.canvas = document.createElement('canvas');
    this.canvas.className = 'graph-canvas';
    this.container.appendChild(this.canvas);
    this.ctx = this.canvas.getContext('2d')!;

    this.setupSize();
    this.resizeObs = new ResizeObserver(() => this.setupSize());
    this.resizeObs.observe(this.container);

    this.setData(data, opts.centerPath);
    this.bindEvents();
    this.startLoop();
  }

  private setupSize(): void {
    const rect = this.container.getBoundingClientRect();
    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    const w = Math.max(200, Math.floor(rect.width || 400));
    const h = Math.max(180, Math.floor(rect.height || 300));
    this.canvas.width = w * dpr;
    this.canvas.height = h * dpr;
    this.canvas.style.width = `${w}px`;
    this.canvas.style.height = `${h}px`;
    this.ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  }

  public setTheme(theme: 'light' | 'dark'): void {
    this.opts.theme = theme;
  }

  public setSearchQuery(query: string): void {
    this.opts.searchQuery = query;
  }

  public setLayoutMode(mode: GraphLayoutMode): void {
    if (this.layoutMode === mode) return;
    this.layoutMode = mode;
    this.initializePositions();
    this.animTicks = 0;
    this.fitToScreen();
  }

  public setHideOrphans(hide: boolean): void {
    if (this.hideOrphans === hide) return;
    this.hideOrphans = hide;
    this.rebuildVisibleSubgraph();
    this.initializePositions();
    this.animTicks = 0;
    this.fitToScreen();
  }

  public setFolderFilter(folder: string): void {
    if (this.folderFilter === folder) return;
    this.folderFilter = folder;
    this.rebuildVisibleSubgraph();
    this.initializePositions();
    this.animTicks = 0;
    this.fitToScreen();
  }

  public selectNodeById(id: string | null): void {
    this.selectedId = id;
    const node = id ? this.nodeById.get(id) || null : null;
    if (node) {
      this.emitSelection(node);
    }
  }

  public resetCamera(): void {
    this.fitToScreen();
  }

  public fitToScreen(): void {
    const nodes = this.visibleNodes;
    if (nodes.length === 0) {
      this.camX = 0;
      this.camY = 0;
      this.scale = 1;
      return;
    }
    const w = this.canvas.clientWidth || 600;
    const h = this.canvas.clientHeight || 400;
    const rightReserve = !this.opts.compact && w > 820 ? 290 : 0;
    const usableW = Math.max(220, w - rightReserve - 80);
    const usableH = Math.max(180, h - 80);

    let minX = Infinity;
    let maxX = -Infinity;
    let minY = Infinity;
    let maxY = -Infinity;
    for (const n of nodes) {
      minX = Math.min(minX, n.x - 45);
      maxX = Math.max(maxX, n.x + 45);
      minY = Math.min(minY, n.y - 30);
      maxY = Math.max(maxY, n.y + 30);
    }
    const spanW = Math.max(140, maxX - minX);
    const spanH = Math.max(140, maxY - minY);
    const nextScale = Math.min(1.45, Math.max(0.28, Math.min(usableW / spanW, usableH / spanH)));
    const midX = (minX + maxX) / 2;
    const midY = (minY + maxY) / 2;

    this.scale = nextScale;
    this.camX = -midX * nextScale - rightReserve * 0.35;
    this.camY = -midY * nextScale;
  }

  public setData(data: GraphDto, centerPath?: string | null): void {
    this.opts.centerPath = centerPath ?? null;
    const prevPos = new Map<string, { x: number; y: number }>();
    for (const n of this.allNodes) {
      prevPos.set(n.id, { x: n.x, y: n.y });
    }

    this.allNodes = data.nodes.map((n) => {
      const old = prevPos.get(n.id);
      const isCenter = Boolean(centerPath && n.path === centerPath);
      const baseR =
        n.kind === 'tag'
          ? 6
          : n.kind === 'ghost'
            ? 6.5
            : Math.min(16, 7.5 + Math.sqrt(Math.max(1, n.degree)) * 2.2);
      return {
        ...n,
        x: old?.x ?? 0,
        y: old?.y ?? 0,
        vx: 0,
        vy: 0,
        radius: isCenter ? baseR + 3 : baseR,
      };
    });
    this.allEdges = data.edges.map((e) => ({ ...e }));

    // Assign stable distinct colors per folder cluster
    this.folderColors.clear();
    const groups = Array.from(
      new Set(
        this.allNodes
          .filter((n) => n.kind === 'note')
          .map((n) => n.folder || n.group || 'root')
      )
    ).sort();
    groups.forEach((g, idx) => {
      this.folderColors.set(g, CLUSTER_PALETTE[idx % CLUSTER_PALETTE.length]);
    });

    this.rebuildVisibleSubgraph();
    this.initializePositions();
    this.animTicks = 0;
    this.fitToScreen();
  }

  private rebuildVisibleSubgraph(): void {
    let nodes = [...this.allNodes];
    if (this.folderFilter) {
      const ff = this.folderFilter;
      const inFolder = new Set<string>();
      for (const n of nodes) {
        if (n.kind !== 'note') continue;
        const nf = n.folder || '';
        if (ff === '__root__' ? nf === '' : nf === ff || nf.startsWith(`${ff}/`)) {
          inFolder.add(n.id);
        }
      }
      // Also include directly connected nodes for context
      const expanded = new Set(inFolder);
      for (const e of this.allEdges) {
        if (inFolder.has(e.source)) expanded.add(e.target);
        if (inFolder.has(e.target)) expanded.add(e.source);
      }
      nodes = nodes.filter((n) => expanded.has(n.id));
    }

    if (this.hideOrphans) {
      nodes = nodes.filter(
        (n) => n.degree > 0 || (this.opts.centerPath && n.path === this.opts.centerPath)
      );
    }

    const keepIds = new Set(nodes.map((n) => n.id));
    this.visibleNodes = nodes;
    this.visibleEdges = this.allEdges.filter(
      (e) => keepIds.has(e.source) && keepIds.has(e.target)
    );
    this.nodeById.clear();
    for (const n of this.visibleNodes) {
      this.nodeById.set(n.id, n);
    }
    if (this.selectedId && !this.nodeById.has(this.selectedId)) {
      this.selectedId = null;
      this.opts.onNodeSelect?.(null, { inbound: [], outbound: [] });
    }
  }

  private getClusterCenters(): Map<string, { x: number; y: number; color: string }> {
    const map = new Map<string, { x: number; y: number; color: string }>();
    const folders = Array.from(
      new Set(
        this.visibleNodes.map((n) =>
          n.kind === 'note' ? n.folder || '루트 노트' : n.kind === 'tag' ? '태그' : '미생성 노트'
        )
      )
    ).sort();
    const count = Math.max(1, folders.length);
    const ringR = count === 1 ? 0 : Math.max(180, 120 + count * 34);
    folders.forEach((f, idx) => {
      const ang = (idx / count) * Math.PI * 2 - Math.PI / 2;
      map.set(f, {
        x: Math.cos(ang) * ringR,
        y: Math.sin(ang) * ringR,
        color:
          this.folderColors.get(f === '루트 노트' ? 'root' : f) ||
          CLUSTER_PALETTE[idx % CLUSTER_PALETTE.length],
      });
    });
    return map;
  }

  private initializePositions(): void {
    const nodes = this.visibleNodes;
    if (nodes.length === 0) return;

    if (this.layoutMode === 'radial') {
      const anchor =
        (this.selectedId && this.nodeById.get(this.selectedId)) ||
        nodes.find((n) => this.opts.centerPath && n.path === this.opts.centerPath) ||
        nodes.slice().sort((a, b) => b.degree - a.degree)[0];

      const adj = new Map<string, string[]>();
      for (const e of this.visibleEdges) {
        if (!adj.has(e.source)) adj.set(e.source, []);
        if (!adj.has(e.target)) adj.set(e.target, []);
        adj.get(e.source)!.push(e.target);
        adj.get(e.target)!.push(e.source);
      }

      const hopMap = new Map<string, number>();
      if (anchor) {
        const q: [string, number][] = [[anchor.id, 0]];
        hopMap.set(anchor.id, 0);
        while (q.length > 0) {
          const [cur, d] = q.shift()!;
          for (const nb of adj.get(cur) || []) {
            if (!hopMap.has(nb)) {
              hopMap.set(nb, d + 1);
              q.push([nb, d + 1]);
            }
          }
        }
      }

      const byHop = new Map<number, SimNode[]>();
      for (const n of nodes) {
        const h = hopMap.has(n.id) ? hopMap.get(n.id)! : 4;
        n.hop = h;
        if (!byHop.has(h)) byHop.set(h, []);
        byHop.get(h)!.push(n);
      }

      for (const [h, grp] of byHop) {
        grp.sort((a, b) => a.group.localeCompare(b.group) || a.label.localeCompare(b.label));
        if (h === 0) {
          grp[0].x = 0;
          grp[0].y = 0;
        } else {
          const radius = Math.max(h * 155, (grp.length * 64) / (Math.PI * 2));
          grp.forEach((n, i) => {
            const ang = (i / grp.length) * Math.PI * 2 - Math.PI / 2;
            n.x = Math.cos(ang) * radius;
            n.y = Math.sin(ang) * radius;
          });
        }
      }
      nodes.forEach((n) => {
        n.vx = 0;
        n.vy = 0;
      });
      return;
    }

    if (this.layoutMode === 'hierarchy') {
      const outAdj = new Map<string, string[]>();
      for (const e of this.visibleEdges) {
        if (!outAdj.has(e.source)) outAdj.set(e.source, []);
        outAdj.get(e.source)!.push(e.target);
      }

      const rankMap = new Map<string, number>();
      const roots = nodes
        .filter((n) => n.kind === 'note' && ((n.inbound_count ?? 0) === 0 || (this.opts.centerPath && n.path === this.opts.centerPath)))
        .sort((a, b) => b.degree - a.degree);
      const seeds = roots.length > 0 ? roots : nodes.slice().sort((a, b) => b.degree - a.degree).slice(0, 3);
      const q: string[] = [];
      for (const r of seeds) {
        rankMap.set(r.id, 0);
        q.push(r.id);
      }
      while (q.length > 0) {
        const cur = q.shift()!;
        const r = rankMap.get(cur)!;
        for (const dst of outAdj.get(cur) || []) {
          if (!rankMap.has(dst)) {
            rankMap.set(dst, r + 1);
            q.push(dst);
          }
        }
      }

      let maxR = 0;
      for (const v of rankMap.values()) maxR = Math.max(maxR, v);
      const byRank = new Map<number, SimNode[]>();
      for (const n of nodes) {
        const rk = rankMap.has(n.id) ? rankMap.get(n.id)! : n.kind === 'tag' ? maxR + 1 : maxR + 1;
        n.rank = rk;
        if (!byRank.has(rk)) byRank.set(rk, []);
        byRank.get(rk)!.push(n);
      }

      const ranks = Array.from(byRank.keys()).sort((a, b) => a - b);
      const colSpacing = 240;
      const totalCols = Math.max(1, ranks.length - 1);
      ranks.forEach((rk, colIdx) => {
        const list = byRank
          .get(rk)!
          .sort((a, b) => (a.folder || a.group).localeCompare(b.folder || b.group) || a.label.localeCompare(b.label));
        const rowSpacing = Math.max(66, Math.min(110, 560 / Math.max(1, list.length)));
        list.forEach((n, rowIdx) => {
          n.x = (colIdx - totalCols / 2) * colSpacing;
          n.y = (rowIdx - (list.length - 1) / 2) * rowSpacing;
          n.vx = 0;
          n.vy = 0;
        });
      });
      return;
    }

    // Cluster mode initial placement: spread cleanly around folder cluster center
    const centers = this.getClusterCenters();
    const bucketCounts = new Map<string, number>();
    for (const n of nodes) {
      const key =
        n.kind === 'note' ? n.folder || '루트 노트' : n.kind === 'tag' ? '태그' : '미생성 노트';
      const idx = bucketCounts.get(key) || 0;
      bucketCounts.set(key, idx + 1);
      const c = centers.get(key) || { x: 0, y: 0, color: '#2563eb' };
      const localRing = 48 + Math.floor(idx / 6) * 56;
      const ang = (idx % 6) * (Math.PI / 3) + (hashString(n.id) % 100) * 0.01;
      n.x = c.x + Math.cos(ang) * localRing;
      n.y = c.y + Math.sin(ang) * localRing;
      n.vx = 0;
      n.vy = 0;
    }
  }

  private emitSelection(node: GraphNodeDto | null): void {
    if (!this.opts.onNodeSelect) return;
    if (!node) {
      this.opts.onNodeSelect(null, { inbound: [], outbound: [] });
      return;
    }
    const inbound: GraphNodeDto[] = [];
    const outbound: GraphNodeDto[] = [];
    const seenIn = new Set<string>();
    const seenOut = new Set<string>();
    for (const e of this.allEdges) {
      if (e.source === node.id && !seenOut.has(e.target)) {
        const t = this.allNodes.find((x) => x.id === e.target);
        if (t) {
          seenOut.add(t.id);
          outbound.push(t);
        }
      }
      if (e.target === node.id && !seenIn.has(e.source)) {
        const s = this.allNodes.find((x) => x.id === e.source);
        if (s) {
          seenIn.add(s.id);
          inbound.push(s);
        }
      }
    }
    this.opts.onNodeSelect(node, { inbound, outbound });
  }

  private stepPhysics(): void {
    const nodes = this.visibleNodes;
    if (nodes.length === 0) return;

    if (this.layoutMode === 'cluster') {
      const centers = this.getClusterCenters();
      const repulsion = this.opts.compact ? 3800 : 7200;
      const damping = 0.82;

      for (let i = 0; i < nodes.length; i++) {
        const a = nodes[i];
        const keyA =
          a.kind === 'note' ? a.folder || '루트 노트' : a.kind === 'tag' ? '태그' : '미생성 노트';
        const c = centers.get(keyA);
        if (c) {
          a.vx += (c.x - a.x) * 0.018;
          a.vy += (c.y - a.y) * 0.018;
        } else {
          a.vx += -a.x * 0.008;
          a.vy += -a.y * 0.008;
        }

        for (let j = i + 1; j < nodes.length; j++) {
          const b = nodes[j];
          const dx = b.x - a.x;
          const dy = b.y - a.y;
          const distSq = dx * dx + dy * dy + 64;
          const dist = Math.sqrt(distSq);
          const f = Math.min(18, repulsion / distSq);
          const fx = (dx / dist) * f;
          const fy = (dy / dist) * f;
          a.vx -= fx;
          a.vy -= fy;
          b.vx += fx;
          b.vy += fy;
        }
      }

      for (const e of this.visibleEdges) {
        const s = this.nodeById.get(e.source);
        const t = this.nodeById.get(e.target);
        if (!s || !t) continue;
        const dx = t.x - s.x;
        const dy = t.y - s.y;
        const dist = Math.sqrt(dx * dx + dy * dy) || 1;
        const sameFolder = s.kind === 'note' && t.kind === 'note' && s.folder === t.folder;
        const rest = this.opts.compact ? 82 : sameFolder ? 115 : 195;
        const f = (dist - rest) * 0.014;
        const fx = (dx / dist) * f;
        const fy = (dy / dist) * f;
        s.vx += fx;
        s.vy += fy;
        t.vx -= fx;
        t.vy -= fy;
      }

      for (const n of nodes) {
        if (n.pinned) {
          n.vx = 0;
          n.vy = 0;
          continue;
        }
        n.vx *= damping;
        n.vy *= damping;
        n.x += Math.max(-16, Math.min(16, n.vx));
        n.y += Math.max(-16, Math.min(16, n.vy));
      }
    }

    // Strict pairwise node & label anti-clump collision resolution in ALL modes
    const padRadius = this.opts.compact ? 22 : 40;
    for (let iter = 0; iter < 3; iter++) {
      for (let i = 0; i < nodes.length; i++) {
        const a = nodes[i];
        for (let j = i + 1; j < nodes.length; j++) {
          const b = nodes[j];
          const dx = b.x - a.x;
          const dy = b.y - a.y;
          const dist = Math.sqrt(dx * dx + dy * dy) || 0.01;
          const minDist = a.radius + b.radius + padRadius;
          if (dist < minDist) {
            const push = (minDist - dist) * 0.45;
            const nx = dx / dist;
            const ny = dy / dist;
            if (!a.pinned) {
              a.x -= nx * push;
              a.y -= ny * push;
            }
            if (!b.pinned) {
              b.x += nx * push;
              b.y += ny * push;
            }
          }
          // Vertical/horizontal label box anti-overlap when zoom is comfortable
          if (!this.opts.compact && Math.abs(dx) < 88 && Math.abs(dy) < 26) {
            const signY = dy >= 0 ? 1 : -1;
            const pushY = (26 - Math.abs(dy)) * 0.28;
            if (!a.pinned) a.y -= signY * pushY;
            if (!b.pinned) b.y += signY * pushY;
          }
        }
      }
    }
  }

  private screenToWorld(sx: number, sy: number): { x: number; y: number } {
    const w = this.canvas.clientWidth;
    const h = this.canvas.clientHeight;
    return {
      x: (sx - w / 2 - this.camX) / this.scale,
      y: (sy - h / 2 - this.camY) / this.scale,
    };
  }

  private hitTest(sx: number, sy: number): SimNode | null {
    const pt = this.screenToWorld(sx, sy);
    let best: SimNode | null = null;
    let bestDist = Infinity;
    for (const n of this.visibleNodes) {
      const dx = pt.x - n.x;
      const dy = pt.y - n.y;
      const d = Math.sqrt(dx * dx + dy * dy);
      const hitR = n.radius + 7 / Math.max(0.4, this.scale);
      if (d <= hitR && d < bestDist) {
        best = n;
        bestDist = d;
      }
    }
    return best;
  }

  private drawArrow(
    ctx: CanvasRenderingContext2D,
    x1: number,
    y1: number,
    x2: number,
    y2: number,
    targetRadius: number,
    color: string,
    lineWidth: number
  ): void {
    const dx = x2 - x1;
    const dy = y2 - y1;
    const len = Math.sqrt(dx * dx + dy * dy);
    if (len < targetRadius + 10) return;

    const ux = dx / len;
    const uy = dy / len;
    const tipX = x2 - ux * (targetRadius + 3);
    const tipY = y2 - uy * (targetRadius + 3);
    const headLen = Math.min(9, 5 + lineWidth * 1.4);
    const angle = Math.atan2(dy, dx);

    ctx.beginPath();
    if (this.layoutMode === 'hierarchy') {
      const cx1 = x1 + (x2 - x1) * 0.45;
      const cy1 = y1;
      const cx2 = x2 - (x2 - x1) * 0.45;
      const cy2 = y2;
      ctx.moveTo(x1, y1);
      ctx.bezierCurveTo(cx1, cy1, cx2, cy2, tipX, tipY);
    } else {
      ctx.moveTo(x1, y1);
      ctx.lineTo(tipX, tipY);
    }
    ctx.strokeStyle = color;
    ctx.lineWidth = lineWidth;
    ctx.stroke();

    ctx.beginPath();
    ctx.moveTo(tipX, tipY);
    ctx.lineTo(
      tipX - headLen * Math.cos(angle - Math.PI / 7),
      tipY - headLen * Math.sin(angle - Math.PI / 7)
    );
    ctx.lineTo(
      tipX - headLen * Math.cos(angle + Math.PI / 7),
      tipY - headLen * Math.sin(angle + Math.PI / 7)
    );
    ctx.closePath();
    ctx.fillStyle = color;
    ctx.fill();
  }

  private draw(): void {
    const ctx = this.ctx;
    const w = this.canvas.clientWidth;
    const h = this.canvas.clientHeight;
    const isDark = this.opts.theme === 'dark';

    ctx.clearRect(0, 0, w, h);

    if (this.visibleNodes.length === 0) {
      ctx.fillStyle = isDark ? '#9ca3af' : '#6b7280';
      ctx.font = '13px sans-serif';
      ctx.textAlign = 'center';
      ctx.fillText('표시할 그래프 노드가 없습니다.', w / 2, h / 2);
      return;
    }

    ctx.save();
    ctx.translate(w / 2 + this.camX, h / 2 + this.camY);
    ctx.scale(this.scale, this.scale);

    // 1. Structural Background Guides per Mode
    if (this.layoutMode === 'cluster' && !this.opts.compact) {
      const groups = new Map<string, { nodes: SimNode[]; color: string }>();
      let colorIdx = 0;
      for (const n of this.visibleNodes) {
        if (n.kind !== 'note') continue;
        const gKey = n.folder || '루트 노트';
        let entry = groups.get(gKey);
        if (!entry) {
          entry = {
            nodes: [],
            color:
              this.folderColors.get(n.folder || 'root') ||
              CLUSTER_PALETTE[colorIdx++ % CLUSTER_PALETTE.length],
          };
          groups.set(gKey, entry);
        }
        entry.nodes.push(n);
      }

      for (const [gKey, entry] of groups) {
        if (entry.nodes.length === 0) continue;
        let cx = 0;
        let cy = 0;
        for (const n of entry.nodes) {
          cx += n.x;
          cy += n.y;
        }
        cx /= entry.nodes.length;
        cy /= entry.nodes.length;
        let maxR = 46;
        for (const n of entry.nodes) {
          const d = Math.hypot(n.x - cx, n.y - cy) + n.radius + 34;
          if (d > maxR) maxR = d;
        }

        ctx.save();
        ctx.beginPath();
        ctx.arc(cx, cy, maxR, 0, Math.PI * 2);
        ctx.fillStyle = `${entry.color}${isDark ? '18' : '10'}`;
        ctx.fill();
        ctx.setLineDash([5, 4]);
        ctx.strokeStyle = `${entry.color}${isDark ? '55' : '40'}`;
        ctx.lineWidth = 1.2 / this.scale;
        ctx.stroke();
        ctx.setLineDash([]);

        ctx.font = `600 ${Math.max(10.5, 11.5 / Math.max(0.6, this.scale))}px sans-serif`;
        ctx.textAlign = 'center';
        ctx.fillStyle = entry.color;
        ctx.fillText(`📁 ${gKey} (${entry.nodes.length})`, cx, cy - maxR - 8);
        ctx.restore();
      }
    } else if (this.layoutMode === 'radial' && !this.opts.compact) {
      const ringLabels = ['1촌 직접 연결', '2촌 간접 연결', '3촌 확장'];
      [155, 310, 465].forEach((r, idx) => {
        ctx.beginPath();
        ctx.arc(0, 0, r, 0, Math.PI * 2);
        ctx.setLineDash([4, 5]);
        ctx.strokeStyle = isDark ? 'rgba(148,163,184,0.18)' : 'rgba(100,116,139,0.2)';
        ctx.lineWidth = 1 / this.scale;
        ctx.stroke();
        ctx.setLineDash([]);
        ctx.font = '10.5px sans-serif';
        ctx.textAlign = 'center';
        ctx.fillStyle = isDark ? 'rgba(148,163,184,0.55)' : 'rgba(100,116,139,0.65)';
        ctx.fillText(ringLabels[idx], 0, -r - 6);
      });
    }

    const q = (this.opts.searchQuery || '').trim().toLowerCase();
    const activeFocus = this.hoveredId || this.selectedId;
    const adjacentIds = new Set<string>();
    if (activeFocus) {
      adjacentIds.add(activeFocus);
      for (const e of this.visibleEdges) {
        if (e.source === activeFocus) adjacentIds.add(e.target);
        if (e.target === activeFocus) adjacentIds.add(e.source);
      }
    }

    // 2. Draw Edges with Directional Arrows & Out/In Distinction
    for (const e of this.visibleEdges) {
      const s = this.nodeById.get(e.source);
      const t = this.nodeById.get(e.target);
      if (!s || !t) continue;

      const isOut = Boolean(activeFocus && e.source === activeFocus);
      const isIn = Boolean(activeFocus && e.target === activeFocus);
      const isDimmed = Boolean(activeFocus && !isOut && !isIn);

      let strokeColor = isDark ? 'rgba(148, 163, 184, 0.24)' : 'rgba(100, 116, 139, 0.28)';
      let width = 1.1 / Math.max(0.5, this.scale);
      if (isOut) {
        strokeColor = '#3b82f6';
        width = 2.1 / Math.max(0.5, this.scale);
      } else if (isIn) {
        strokeColor = '#10b981';
        width = 2.1 / Math.max(0.5, this.scale);
      } else if (isDimmed) {
        strokeColor = isDark ? 'rgba(148, 163, 184, 0.08)' : 'rgba(100, 116, 139, 0.09)';
      }

      this.drawArrow(ctx, s.x, s.y, t.x, t.y, t.radius, strokeColor, width);
    }

    // 3. Draw Nodes & Clear Non-Overlapping Labels
    const showAllLabels = this.scale >= 0.45 || this.visibleNodes.length <= 45;
    for (const n of this.visibleNodes) {
      const isCenter = Boolean(this.opts.centerPath && n.path === this.opts.centerPath);
      const isHovered = this.hoveredId === n.id;
      const isSelected = this.selectedId === n.id;
      const isSearchHit = Boolean(q && n.label.toLowerCase().includes(q));
      const isDimmed = Boolean(activeFocus && !adjacentIds.has(n.id));

      ctx.save();
      if (isDimmed && !isSearchHit) {
        ctx.globalAlpha = 0.24;
      }

      let fill = this.folderColors.get(n.folder || 'root') || '#2383e2';
      if (n.kind === 'tag') fill = '#0f7b6c';
      else if (n.kind === 'ghost') fill = '#9b9a97';
      if (isCenter) fill = '#e03e3e';
      if (isSearchHit) fill = '#f59e0b';

      if (isSelected || isHovered || isSearchHit || isCenter) {
        ctx.beginPath();
        ctx.arc(n.x, n.y, n.radius + 4.5, 0, Math.PI * 2);
        ctx.fillStyle = isSelected
          ? 'rgba(37, 99, 235, 0.22)'
          : isCenter
            ? 'rgba(224, 62, 62, 0.22)'
            : 'rgba(245, 158, 11, 0.25)';
        ctx.fill();
      }

      ctx.beginPath();
      ctx.arc(n.x, n.y, n.radius, 0, Math.PI * 2);
      ctx.fillStyle = fill;
      ctx.fill();
      ctx.lineWidth = (isSelected ? 2.4 : 1.3) / Math.max(0.5, this.scale);
      ctx.strokeStyle = isSelected
        ? '#ffffff'
        : isDark
          ? '#1e222a'
          : '#ffffff';
      ctx.stroke();

      if (showAllLabels || isHovered || isSelected || isSearchHit || isCenter) {
        const fontSize = Math.max(
          9.5,
          (this.opts.compact ? 10.5 : 11.5) / Math.max(0.65, this.scale * 0.92)
        );
        ctx.font = `${isSelected || isCenter || isHovered ? '700 ' : '500 '}${fontSize}px Pretendard, Malgun Gothic, sans-serif`;
        ctx.textAlign = 'center';
        ctx.textBaseline = 'top';
        const labelText = n.icon && n.kind === 'note' ? `${n.icon} ${n.label}` : n.label;
        const labelY = n.y + n.radius + 4;
        const metrics = ctx.measureText(labelText);
        const padX = 5;
        ctx.fillStyle = isDark ? 'rgba(25, 29, 36, 0.84)' : 'rgba(255, 255, 255, 0.86)';
        ctx.fillRect(
          n.x - metrics.width / 2 - padX,
          labelY - 1,
          metrics.width + padX * 2,
          fontSize + 3
        );
        ctx.fillStyle = isDark ? '#f1f5f9' : '#0f172a';
        ctx.fillText(labelText, n.x, labelY);
      }

      ctx.restore();
    }

    ctx.restore();
  }

  private startLoop(): void {
    const tick = () => {
      if (this.animTicks < 220 || this.dragNode) {
        this.stepPhysics();
        this.animTicks++;
        if (this.animTicks === 32 && !this.dragNode && !this.isPanning) {
          this.fitToScreen();
        }
      }
      this.draw();
      this.rafId = requestAnimationFrame(tick);
    };
    this.rafId = requestAnimationFrame(tick);
  }

  private bindEvents(): void {
    const getXY = (ev: PointerEvent) => {
      const r = this.canvas.getBoundingClientRect();
      return { x: ev.clientX - r.left, y: ev.clientY - r.top };
    };

    const onPointerDown = (ev: PointerEvent) => {
      const { x, y } = getXY(ev);
      this.downX = x;
      this.downY = y;
      this.lastMouseX = x;
      this.lastMouseY = y;
      this.dragMoved = false;
      const hit = this.hitTest(x, y);
      if (hit) {
        this.dragNode = hit;
        hit.pinned = true;
      } else {
        this.isPanning = true;
      }
      this.canvas.setPointerCapture?.(ev.pointerId);
    };

    const onPointerMove = (ev: PointerEvent) => {
      const { x, y } = getXY(ev);
      const dx = x - this.lastMouseX;
      const dy = y - this.lastMouseY;
      if (Math.hypot(x - this.downX, y - this.downY) > 4) {
        this.dragMoved = true;
      }
      this.lastMouseX = x;
      this.lastMouseY = y;

      if (this.dragNode) {
        const pt = this.screenToWorld(x, y);
        this.dragNode.x = pt.x;
        this.dragNode.y = pt.y;
        this.animTicks = Math.min(this.animTicks, 180);
      } else if (this.isPanning) {
        this.camX += dx;
        this.camY += dy;
      } else {
        const hit = this.hitTest(x, y);
        this.hoveredId = hit?.id ?? null;
        this.canvas.style.cursor = hit ? 'pointer' : 'grab';
      }
    };

    const onPointerUp = (ev: PointerEvent) => {
      if (this.dragNode) {
        const dropped = this.dragNode;
        dropped.pinned = false;
        this.dragNode = null;
        if (!this.dragMoved) {
          this.selectedId = dropped.id;
          if (this.opts.compact) {
            this.opts.onNodeClick?.(dropped);
          } else {
            this.emitSelection(dropped);
            const now = Date.now();
            if (this.lastTapNodeId === dropped.id && now - this.lastTapTime < 360) {
              this.lastTapNodeId = null;
              this.opts.onNodeOpen?.(dropped);
              if (!this.opts.onNodeOpen && !this.opts.onNodeSelect) {
                this.opts.onNodeClick?.(dropped);
              }
            } else {
              this.lastTapNodeId = dropped.id;
              this.lastTapTime = now;
              if (!this.opts.onNodeSelect) {
                this.opts.onNodeClick?.(dropped);
              }
            }
          }
        }
      } else if (this.isPanning && !this.dragMoved) {
        if (!this.opts.compact) {
          this.selectedId = null;
          this.emitSelection(null);
        }
      }
      this.isPanning = false;
      this.canvas.releasePointerCapture?.(ev.pointerId);
    };

    const onWheel = (ev: WheelEvent) => {
      ev.preventDefault();
      const factor = ev.deltaY < 0 ? 1.1 : 0.9;
      const next = Math.min(3.5, Math.max(0.22, this.scale * factor));
      this.scale = next;
    };

    this.canvas.addEventListener('pointerdown', onPointerDown);
    this.canvas.addEventListener('pointermove', onPointerMove);
    this.canvas.addEventListener('pointerup', onPointerUp);
    this.canvas.addEventListener('pointercancel', onPointerUp);
    this.canvas.addEventListener('wheel', onWheel, { passive: false });
  }

  public destroy(): void {
    if (this.rafId !== null) {
      cancelAnimationFrame(this.rafId);
      this.rafId = null;
    }
    this.resizeObs?.disconnect();
    this.resizeObs = null;
    this.container.innerHTML = '';
  }
}
