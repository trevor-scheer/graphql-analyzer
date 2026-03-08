import * as net from "net";
import { workspace, window, commands, OutputChannel, Disposable } from "vscode";

export interface OtelConfig {
  enabled: boolean;
  endpoint: string;
}

/** Read OTEL settings from the extension configuration. */
export function getOtelConfig(): OtelConfig {
  const config = workspace.getConfiguration("graphql-analyzer");
  return {
    enabled: config.get<boolean>("debug.otelEnabled", false),
    endpoint: config.get<string>("debug.otelEndpoint", "http://localhost:4317"),
  };
}

/** Build environment variables for the LSP server when OTEL is enabled. */
export function buildOtelEnv(otel: OtelConfig): Record<string, string> {
  if (!otel.enabled) {
    return {};
  }
  return {
    OTEL_TRACES_ENABLED: "1",
    OTEL_EXPORTER_OTLP_ENDPOINT: otel.endpoint,
  };
}

/**
 * Register a config change listener that prompts for a server restart
 * when OTEL or log level settings change.
 */
export function registerOtelConfigListener(): Disposable {
  return workspace.onDidChangeConfiguration((event) => {
    if (
      event.affectsConfiguration("graphql-analyzer.debug.logLevel") ||
      event.affectsConfiguration("graphql-analyzer.debug.otelEnabled")
    ) {
      window
        .showInformationMessage(
          "graphql-analyzer: Tracing settings changed. Restart the server to apply.",
          "Restart",
        )
        .then((choice) => {
          if (choice === "Restart") {
            commands.executeCommand("graphql-analyzer.restartServer");
          }
        });
    }
  });
}

/** Register the "Test OpenTelemetry Connection" command. */
export function registerTestOtelCommand(outputChannel: OutputChannel): Disposable {
  return commands.registerCommand("graphql-analyzer.testOtelConnection", async () => {
    const otel = getOtelConfig();

    if (!otel.enabled) {
      const choice = await window.showWarningMessage(
        "OpenTelemetry is not enabled. Enable it in settings?",
        "Open Settings",
      );
      if (choice) {
        commands.executeCommand("workbench.action.openSettings", "graphql-analyzer.debug.otelEnabled");
      }
      return;
    }

    let host: string;
    let port: number;
    try {
      const url = new URL(otel.endpoint);
      host = url.hostname;
      port = parseInt(url.port, 10) || 4317;
    } catch {
      window.showErrorMessage(`Invalid OTEL endpoint URL: ${otel.endpoint}`);
      return;
    }

    outputChannel.appendLine(`[OTEL] Testing connection to ${host}:${port}...`);

    const connected = await new Promise<boolean>((resolve) => {
      const socket = net.createConnection({ host, port, timeout: 3000 }, () => {
        socket.destroy();
        resolve(true);
      });
      socket.on("error", () => {
        socket.destroy();
        resolve(false);
      });
      socket.on("timeout", () => {
        socket.destroy();
        resolve(false);
      });
    });

    if (connected) {
      outputChannel.appendLine(`[OTEL] Connection successful to ${host}:${port}`);
      window.showInformationMessage(`OTLP collector is reachable at ${host}:${port}`);
    } else {
      outputChannel.appendLine(`[OTEL] Connection failed to ${host}:${port}`);
      window.showErrorMessage(
        `Cannot reach OTLP collector at ${host}:${port}. ` +
          `Verify the collector is running and the endpoint is correct.`,
      );
    }
  });
}
