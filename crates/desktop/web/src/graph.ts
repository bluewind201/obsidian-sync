import type { GraphDto, GraphNodeDto } from './types';

interface SimNode extends GraphNodeDto {
  x: number;
  y: number;
  vx: number;
  vy: number;
  radius: number;
  pinned?: boolean;
}

interface SimEdge {
  source: SimNode;
  target: SimNode;
}

export interface ForceGraphOptions {
  centerPath?: string | null;
  theme: 'light' | 'dark';
  compact?: boolean;
  searchQuery?: string;
  onNodeClick: (node: GraphNodeDto) => void;
}

export class ForceGraphCanvas {
  private canvas: HTMLCanvasElement;
  private ctx: CanvasRenderingContext2D;
  private nodes: SimNode[] = [];
  private edges: SimEdge[] = [];
  private adj = new Map<string, Set<string>>();
  private opts: ForceGraphOptions;

  private panX = 0;
  private panY = 0;
  private scale = 1;
  private alpha = 1;
  private rafId: number | null = null;
  private resizeObserver: ResizeObserver | null = null;

  private hoveredNode: SimNode | null = null;
  private draggedNode: SimNode | null = null;
  private isPanning = false;
  private pointerDownPos = { x: 0, y: 0 };
  private lastPointerPos = { x: 0, y: 0 };
  private pointerMoved = false;

  constructor(container: HTMLElement, data: GraphDto, opts: ForceGraphOptions) {
    this.opts = opts;
    container.innerHTML = '';
    this.canvas = document.createElement('canvas');
    this.canvas.className = 'graph-canvas';
    container.appendChild(this.canvas);
    const context = this.canvas.getContext('2d');
    if (!context) {
      throw new Error('Canvas 2D context unavailable');
    }
    this.ctx = context;

    this.setData(data, opts.centerPath);
    this.bindEvents();
    this.resize();

    this.resizeObserver = new ResizeObserver(() => this.resize());
    this.resizeObserver.observe(container);
    this.startLoop();
  }

  public setData(data: GraphDto, centerPath?: string | null): void {
    this.opts.centerPath = centerPath;
    const prevPos = new Map<string, { x: number; y: number }>();
    for (const n of this.nodes) {
      prevPos.set(n.id, { x: n.x, y: n.y });
    }

    const count = Math.max(data.nodes.length, 1);
    const spread = Math.min(260, 48 * Math.sqrt(count));

    this.nodes = data.nodes.map((n, idx) => {
      const existing = prevPos.get(n.id);
      const angle = (idx / count) * Math.PI * 2;
      const dist = n.path && n.path === centerPath ? 0 : spread * (0.35 + 0.65 * ((idx * 7) % 11) / 11);
      const radius =
        n.kind === 'tag'
          ? 5.5
          : Math.min(15, 5.5 + Math.log2(1 + Math.max(0, n.degree)) * 2.4);
      return {
        ...n,
        x: existing ? existing.x : Math.cos(angle) * dist,
        y: existing ? existing.y : Math.sin(angle) * dist,
        vx: 0,
        vy: 0,
        radius,
      };
    });

    const byId = new Map<string, SimNode>();
    this.adj.clear();
    for (const n of this.nodes) {
      byId.set(n.id, n);
      this.adj.set(n.id, new Set());
    }

    this.edges = [];
    for (const e of data.edges) {
      const s = byId.get(e.source);
      const t = byId.get(e.target);
      if (s && t && s !== t) {
        this.edges.push({ source: s, target: t });
        this.adj.get(s.id)?.add(t.id);
        this.adj.get(t.id)?.add(s.id);
      }
    }

    this.alpha = 1;
  }

  public setSearchQuery(q: string): void {
    this.opts.searchQuery = q;
    this.draw();
  }

  public setTheme(theme: 'light' | 'dark'): void {
    this.opts.theme = theme;
    this.draw();
  }

  public resetCamera(): void {
    this.panX = 0;
    this.panY = 0;
    this.scale = 1;
    this.alpha = Math.max(this.alpha, 0.35);
    this.draw();
  }

  private resize(): void {
    const rect = this.canvas.parentElement?.getBoundingClientRect();
    if (!rect || rect.width <= 0 || rect.height <= 0) return;
    const dpr = window.devicePixelRatio || 1;
    this.canvas.width = Math.floor(rect.width * dpr);
    this.canvas.height = Math.floor(rect.height * dpr);
    this.ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    this.draw();
  }

  private screenToWorld(sx: number, sy: number): { x: number; y: number } {
    const rect = this.canvas.getBoundingClientRect();
    const cx = rect.width / 2 + this.panX;
    const cy = rect.height / 2 + this.panY;
    return {
      x: (sx - cx) / this.scale,
      y: (sy - cy) / this.scale,
    };
  }

  private findNodeAt(sx: number, sy: number): SimNode | null {
    const w = this.screenToWorld(sx, sy);
    for (let i = this.nodes.length - 1; i >= 0; i--) {
      const n = this.nodes[i];
      const hitR = n.radius + 5 / this.scale;
      const dx = w.x - n.x;
      const dy = w.y - n.y;
      if (dx * dx + dy * dy <= hitR * hitR) {
        return n;
      }
    }
    return null;
  }

  private bindEvents(): void {
    this.canvas.addEventListener('mousedown', (ev) => {
      const rect = this.canvas.getBoundingClientRect();
      const sx = ev.clientX - rect.left;
      const sy = ev.clientY - rect.top;
      this.pointerDownPos = { x: sx, y: sy };
      this.lastPointerPos = { x: sx, y: sy };
      this.pointerMoved = false;

      const hit = this.findNodeAt(sx, sy);
      if (hit) {
        this.draggedNode = hit;
        hit.pinned = true;
        this.alpha = Math.max(this.alpha, 0.5);
      } else {
        this.isPanning = true;
      }
    });

    window.addEventListener('mousemove', this.handleWindowMouseMove);
    window.addEventListener('mouseup', this.handleWindowMouseUp);

    this.canvas.addEventListener(
      'wheel',
      (ev) => {
        ev.preventDefault();
        const rect = this.canvas.getBoundingClientRect();
        const sx = ev.clientX - rect.left;
        const sy = ev.clientY - rect.top;
        const before = this.screenToWorld(sx, sy);

        const zoomFactor = ev.deltaY < 0 ? 1.12 : 0.89;
        this.scale = Math.min(4.0, Math.max(0.22, this.scale * zoomFactor));

        const cx = rect.width / 2;
        const cy = rect.height / 2;
        this.panX = sx - cx - before.x * this.scale;
        this.panY = sy - cy - before.y * this.scale;
        this.draw();
      },
      { passive: false }
    );
  }

  private handleWindowMouseMove = (ev: MouseEvent): void => {
    const rect = this.canvas.getBoundingClientRect();
    const sx = ev.clientX - rect.left;
    const sy = ev.clientY - rect.top;

    const moveDist = Math.hypot(sx - this.pointerDownPos.x, sy - this.pointerDownPos.y);
    if (moveDist > 4) {
      this.pointerMoved = true;
    }

    if (this.draggedNode) {
      const w = this.screenToWorld(sx, sy);
      this.draggedNode.x = w.x;
      this.draggedNode.y = w.y;
      this.draggedNode.vx = 0;
      this.draggedNode.vy = 0;
      this.alpha = Math.max(this.alpha, 0.35);
      this.draw();
      return;
    }

    if (this.isPanning) {
      this.panX += sx - this.lastPointerPos.x;
      this.panY += sy - this.lastPointerPos.y;
      this.lastPointerPos = { x: sx, y: sy };
      this.draw();
      return;
    }

    if (sx >= 0 && sy >= 0 && sx <= rect.width && sy <= rect.height) {
      const hit = this.findNodeAt(sx, sy);
      if (hit !== this.hoveredNode) {
        this.hoveredNode = hit;
        this.canvas.style.cursor = hit ? 'pointer' : 'grab';
        this.draw();
      }
    } else if (this.hoveredNode) {
      this.hoveredNode = null;
      this.draw();
    }
  };

  private handleWindowMouseUp = (): void => {
    if (this.draggedNode) {
      const clickedNode = this.draggedNode;
      clickedNode.pinned = false;
      this.draggedNode = null;
      if (!this.pointerMoved) {
        this.opts.onNodeClick(clickedNode);
      }
    }
    this.isPanning = false;
  };

  private stepPhysics(): void {
    if (this.alpha < 0.004 || this.nodes.length === 0) return;

    const repulsion = this.opts.compact ? 1100 : 2100;
    const springLen = this.opts.compact ? 68 : 105;
    const springK = 0.038;
    const gravity = 0.014;

    const nLen = this.nodes.length;
    for (let i = 0; i < nLen; i++) {
      const a = this.nodes[i];
      for (let j = i + 1; j < nLen; j++) {
        const b = this.nodes[j];
        let dx = b.x - a.x;
        let dy = b.y - a.y;
        let distSq = dx * dx + dy * dy;
        if (distSq < 1) {
          dx = (Math.random() - 0.5) * 2;
          dy = (Math.random() - 0.5) * 2;
          distSq = 4;
        }
        if (distSq > 160000) continue;
        const dist = Math.sqrt(distSq);
        const force = (repulsion / distSq) * this.alpha;
        const fx = (dx / dist) * force;
        const fy = (dy / dist) * force;
        if (!a.pinned) {
          a.vx -= fx;
          a.vy -= fy;
        }
        if (!b.pinned) {
          b.vx += fx;
          b.vy += fy;
        }
      }
    }

    for (const e of this.edges) {
      const dx = e.target.x - e.source.x;
      const dy = e.target.y - e.source.y;
      const dist = Math.max(1, Math.hypot(dx, dy));
      const displacement = dist - springLen;
      const force = displacement * springK * this.alpha;
      const fx = (dx / dist) * force;
      const fy = (dy / dist) * force;
      if (!e.source.pinned) {
        e.source.vx += fx;
        e.source.vy += fy;
      }
      if (!e.target.pinned) {
        e.target.vx -= fx;
        e.target.vy -= fy;
      }
    }

    for (const n of this.nodes) {
      if (n.pinned) continue;
      if (this.opts.centerPath && n.path === this.opts.centerPath) {
        n.vx += -n.x * 0.08 * this.alpha;
        n.vy += -n.y * 0.08 * this.alpha;
      } else {
        n.vx += -n.x * gravity * this.alpha;
        n.vy += -n.y * gravity * this.alpha;
      }
      n.vx *= 0.82;
      n.vy *= 0.82;
      n.x += n.vx;
      n.y += n.vy;
    }

    this.alpha *= 0.985;
  }

  private draw(): void {
    const rect = this.canvas.getBoundingClientRect();
    if (rect.width <= 0 || rect.height <= 0) return;

    const ctx = this.ctx;
    const isDark = this.opts.theme === 'dark';
    ctx.clearRect(0, 0, rect.width, rect.height);

    if (this.nodes.length === 0) {
      ctx.fillStyle = isDark ? 'rgba(255,255,255,0.42)' : 'rgba(55,53,47,0.45)';
      ctx.font = '13px Pretendard, Malgun Gothic, sans-serif';
      ctx.textAlign = 'center';
      ctx.fillText('표시할 연결 노트가 없습니다', rect.width / 2, rect.height / 2);
      return;
    }

    ctx.save();
    ctx.translate(rect.width / 2 + this.panX, rect.height / 2 + this.panY);
    ctx.scale(this.scale, this.scale);

    const hovered = this.hoveredNode;
    const hoveredNeighbors = hovered ? this.adj.get(hovered.id) : null;
    const query = (this.opts.searchQuery || '').trim().toLowerCase();

    // Draw edges
    for (const e of this.edges) {
      const isSpotlit =
        hovered && (e.source.id === hovered.id || e.target.id === hovered.id);
      const dimmed = hovered && !isSpotlit;
      ctx.beginPath();
      ctx.moveTo(e.source.x, e.source.y);
      ctx.lineTo(e.target.x, e.target.y);
      if (isSpotlit) {
        ctx.strokeStyle = isDark ? '#529CCA' : '#2383E2';
        ctx.lineWidth = 1.9 / this.scale;
      } else {
        ctx.strokeStyle = dimmed
          ? isDark
            ? 'rgba(255,255,255,0.04)'
            : 'rgba(55,53,47,0.05)'
          : isDark
            ? 'rgba(255,255,255,0.16)'
            : 'rgba(55,53,47,0.16)';
        ctx.lineWidth = 1.1 / this.scale;
      }
      ctx.stroke();
    }

    // Draw nodes
    for (const n of this.nodes) {
      const isCenter = Boolean(this.opts.centerPath && n.path === this.opts.centerPath);
      const isHovered = hovered?.id === n.id;
      const isNeighbor = Boolean(hoveredNeighbors?.has(n.id));
      const matchesQuery = !query || n.label.toLowerCase().includes(query);

      const dimmed =
        (hovered && !isHovered && !isNeighbor) || (Boolean(query) && !matchesQuery);

      let fill = isDark ? '#529CCA' : '#2383E2';
      if (n.kind === 'tag') {
        fill = isDark ? '#4DAB9A' : '#0F7B6C';
      } else if (n.kind === 'ghost') {
        fill = isDark ? 'rgba(255,255,255,0.34)' : '#9B9A97';
      } else if (isCenter) {
        fill = '#E03E3E';
      }

      ctx.globalAlpha = dimmed ? 0.2 : 1;

      if (isCenter || isHovered || (query && matchesQuery)) {
        ctx.beginPath();
        ctx.arc(n.x, n.y, n.radius + 4, 0, Math.PI * 2);
        ctx.fillStyle = isCenter
          ? 'rgba(224, 62, 62, 0.22)'
          : 'rgba(35, 131, 226, 0.24)';
        ctx.fill();
      }

      ctx.beginPath();
      ctx.arc(n.x, n.y, n.radius, 0, Math.PI * 2);
      ctx.fillStyle = fill;
      ctx.fill();

      const showLabel =
        isHovered || isCenter || isNeighbor || this.scale > 0.72 || this.nodes.length <= 45;
      if (showLabel) {
        const fontSize = Math.max(10, Math.min(13, 12 / Math.pow(this.scale, 0.35)));
        ctx.font = `${isHovered || isCenter ? 600 : 500} ${fontSize}px Pretendard, Malgun Gothic, sans-serif`;
        ctx.textAlign = 'center';
        ctx.fillStyle = isDark ? 'rgba(255,255,255,0.88)' : '#37352F';
        ctx.fillText(n.label, n.x, n.y + n.radius + fontSize + 2);
      }
    }

    ctx.globalAlpha = 1;
    ctx.restore();
  }

  private startLoop(): void {
    const tick = () => {
      if (this.alpha >= 0.004) {
        this.stepPhysics();
        this.draw();
      }
      this.rafId = requestAnimationFrame(tick);
    };
    this.rafId = requestAnimationFrame(tick);
  }

  public destroy(): void {
    if (this.rafId !== null) {
      cancelAnimationFrame(this.rafId);
      this.rafId = null;
    }
    this.resizeObserver?.disconnect();
    window.removeEventListener('mousemove', this.handleWindowMouseMove);
    window.removeEventListener('mouseup', this.handleWindowMouseUp);
  }
}
