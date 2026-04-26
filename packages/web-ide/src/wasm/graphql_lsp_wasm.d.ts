// Stub for the wasm-pack generated module. wasm-pack overwrites this when
// `xtask web` runs; until then, this unblocks `tsc --noEmit`.
export default function init(): Promise<void>;
export class Server {
  constructor();
  handleMessage(json: string): string[];
}
