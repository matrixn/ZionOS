/** Zion App SDK 1.0 — global namespace for classic offline scripts. Bridge protocol v1. */
declare namespace ZionApp {
    const protocolVersion = 1;
    type ErrorCode = 'UNAVAILABLE' | 'DENIED' | 'INVALID_ARGUMENT' | 'UNSUPPORTED_VERSION' | 'INVALID_MESSAGE' | 'TIMEOUT' | 'CLOSED' | 'BUSY' | 'REPLAY' | 'UNSUPPORTED_METHOD';
    class ZionError extends Error {
        readonly code: ErrorCode;
        constructor(code: ErrorCode, message: string);
    }
    interface Theme {
        mode: 'dark' | 'light';
        accent: 'sea' | 'blue' | 'violet';
        motion: 'normal' | 'reduced';
    }
    interface Capabilities {
        protocol: 1;
        methods: Record<string, boolean>;
        settingsPersistence: false;
        nativeBroker: false;
        isolation: 'sandboxed-opaque-iframe';
    }
    interface Size {
        width: number;
        height: number;
    }
    interface App {
        readonly id: string;
        readonly instanceId: string;
        window: {
            setTitle(title: string): Promise<void>;
            setSize(size: Size): Promise<Size>;
        };
        notifications: {
            show(options: {
                title: string;
                message: string;
            }): Promise<{
                shown: true;
            }>;
        };
        theme: {
            get(): Promise<Theme>;
            subscribe(listener: (theme: Theme) => void): Promise<() => void>;
        };
        dialog: {
            confirm(options: {
                title: string;
                message: string;
            }): Promise<boolean>;
        };
        /** Explicitly unavailable in v1: no pretend browser-local persistent settings. */
        settings: {
            get(key: string): Promise<never>;
            set(key: string, value: unknown): Promise<never>;
        };
        /** No NAS path, file descriptor or directory handle is exposed in v1. */
        files: {
            pickDirectory(): Promise<never>;
        };
        system: {
            getCapabilities(): Promise<Capabilities>;
        };
        disconnect(): void;
    }
    interface ConnectOptions {
        parentOrigin?: string;
        timeoutMs?: number;
    }
    function connect(options?: ConnectOptions): Promise<App>;
}
