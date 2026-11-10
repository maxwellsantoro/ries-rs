function pkgCandidates(threads) {
  const currentUrl = new URL(import.meta.url);
  const pathname = currentUrl.pathname;
  const servedFromWebDir =
    pathname.includes('/web/') ||
    pathname.endsWith('/web') ||
    pathname.endsWith('/web/index.html');

  const packageDir = threads ? 'pkg-threads' : 'pkg';
  const candidates = [
    new URL((servedFromWebDir ? '../' : './') + packageDir + '/ries_rs.js', currentUrl).href,
  ];

  if (!servedFromWebDir) {
    candidates.push(new URL('../' + packageDir + '/ries_rs.js', currentUrl).href);
  }

  candidates.push(new URL('/' + packageDir + '/ries_rs.js', currentUrl).href);
  return [...new Set(candidates)];
}

let wasmModule = null;
let workerReady = false;
let workerCount = 0;
let threaded = false;

async function loadWasmModule(threads) {
  let lastError = null;

  const candidates = threads ? [...pkgCandidates(true), ...pkgCandidates(false)] : pkgCandidates(false);
  for (const candidate of candidates) {
    try {
      const module = await import(candidate);
      await module.default();
      wasmModule = module;
      return wasmModule;
    } catch (err) {
      lastError = err;
    }
  }

  throw lastError || new Error('Failed to load WASM module');
}

function serializePresets(presets) {
  if (presets instanceof Map) {
    return Array.from(presets.entries());
  }
  if (Array.isArray(presets)) {
    return presets.map(function(name) {
      return [name, ''];
    });
  }
  if (presets && typeof presets === 'object') {
    return Object.entries(presets);
  }
  return [];
}

function serializeMatch(match) {
  return {
    lhs: match.lhs,
    rhs: match.rhs,
    lhs_postfix: match.lhs_postfix,
    rhs_postfix: match.rhs_postfix,
    solve_for_x: match.solve_for_x,
    solve_for_x_postfix: match.solve_for_x_postfix,
    canonical_key: match.canonical_key,
    x_value: match.x_value,
    error: match.error,
    complexity: match.complexity,
    operator_count: match.operator_count,
    tree_depth: match.tree_depth,
    is_exact: match.is_exact,
    constants: match.constants,
  };
}

function serializeMatches(matches) {
  return Array.from(matches || []).map(serializeMatch);
}

async function initialize(concurrency, threads) {
  await loadWasmModule(threads);

  threaded = typeof wasmModule.initThreadPool === 'function';
  if (threaded) {
    try {
      await wasmModule.initThreadPool(concurrency || 4);
      workerCount = concurrency || 4;
    } catch (err) {
      threaded = false;
      workerCount = 0;
      console.warn('Failed to initialize worker thread pool; using single-threaded WASM:', err);
      await loadWasmModule(false);
    }
  }

  workerReady = true;
  self.postMessage({
    type: 'ready',
    version: wasmModule.version ? wasmModule.version() : 'unknown',
    threaded: threaded,
    workerCount: workerCount,
  });
}

async function handleRequest(message) {
  if (!workerReady) {
    throw new Error('WASM worker is not ready');
  }

  switch (message.type) {
    case 'version': {
      return wasmModule.version ? wasmModule.version() : 'unknown';
    }
    case 'listPresets': {
      return serializePresets(wasmModule.listPresets());
    }
    case 'search': {
      const matches = wasmModule.search(message.targetValue, message.searchConfig);
      return serializeMatches(matches);
    }
    default:
      throw new Error('Unknown worker request: ' + message.type);
  }
}

self.addEventListener('message', async function(event) {
  const message = event.data || {};

  if (message.type === 'init') {
    try {
      await initialize(message.concurrency, message.threads);
    } catch (err) {
      self.postMessage({
        type: 'error',
        error: err instanceof Error ? err.message : String(err),
      });
    }
    return;
  }

  if (message.type === 'shutdown') {
    self.close();
    return;
  }

  try {
    const value = await handleRequest(message);
    self.postMessage({
      type: 'response',
      id: message.id,
      value: value,
    });
  } catch (err) {
    self.postMessage({
      type: 'error',
      id: message.id,
      error: err instanceof Error ? err.message : String(err),
    });
  }
});
