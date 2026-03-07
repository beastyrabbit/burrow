import { resolvePortlessConfig } from "./portless-resolver.mjs";

process.stdout.write(`${resolvePortlessConfig().frontendUrl}\n`);
