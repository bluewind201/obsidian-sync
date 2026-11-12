declare module '@toast-ui/editor' {
  export type EditorType = 'markdown' | 'wysiwyg';
  export type PreviewStyle = 'tab' | 'vertical';

  export interface PopupOptions {
    body: HTMLElement;
    className?: string;
    style?: Record<string, unknown>;
  }

  export interface ToolbarCustomOptions {
    name: string;
    tooltip?: string;
    el?: HTMLElement;
    text?: string;
    className?: string;
    command?: string;
    popup?: PopupOptions;
  }

  export interface PluginContext {
    eventEmitter: any;
    i18n: any;
    instance: Editor;
    pmState: any;
    pmView: any;
    pmModel: any;
  }

  export type CommandHandler = (
    payload: Record<string, any> | undefined,
    state: any,
    dispatch: any,
    view?: any
  ) => boolean;

  export interface PluginInfo {
    markdownCommands?: Record<string, CommandHandler>;
    wysiwygCommands?: Record<string, CommandHandler>;
    toHTMLRenderers?: Record<string, any>;
    toMarkdownRenderers?: Record<string, any>;
    toolbarItems?: { groupIndex: number; itemIndex: number; item: string | ToolbarCustomOptions }[];
  }

  export interface EditorOptions {
    el: HTMLElement;
    height?: string;
    minHeight?: string;
    initialValue?: string;
    previewStyle?: PreviewStyle;
    initialEditType?: EditorType;
    language?: string;
    useCommandShortcut?: boolean;
    usageStatistics?: boolean;
    hideModeSwitch?: boolean;
    placeholder?: string;
    theme?: string;
    viewer?: boolean;
    plugins?: any[];
    toolbarItems?: (string | ToolbarCustomOptions)[][];
    hooks?: {
      addImageBlobHook?: (
        blob: Blob | File,
        callback: (url: string, text?: string) => void
      ) => void | Promise<void>;
    };
    events?: {
      load?: (editor: Editor) => void;
      change?: (editorType: EditorType) => void;
      caretChange?: (editorType: EditorType) => void;
      focus?: (editorType: EditorType) => void;
      blur?: (editorType: EditorType) => void;
      keydown?: (editorType: EditorType, ev: KeyboardEvent) => void;
      keyup?: (editorType: EditorType, ev: KeyboardEvent) => void;
      beforePreviewRender?: (html: string) => string;
      beforeConvertWysiwygToMarkdown?: (markdownText: string) => string;
    };
  }

  export default class Editor {
    constructor(options: EditorOptions);
    static factory(options: EditorOptions): Editor;
    exec(name: string, payload?: Record<string, any>): void;
    focus(): void;
    blur(): void;
    getMarkdown(): string;
    setMarkdown(markdown: string, cursorToEnd?: boolean): void;
    getHTML(): string;
    setHTML(html: string, cursorToEnd?: boolean): void;
    insertText(text: string): void;
    isMarkdownMode(): boolean;
    isWysiwygMode(): boolean;
    changeMode(mode: EditorType, isWithoutFocus?: boolean): void;
    destroy(): void;
  }
}

declare module '@toast-ui/editor/dist/i18n/ko-kr';
