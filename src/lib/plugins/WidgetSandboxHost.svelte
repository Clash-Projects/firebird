<script lang="ts">
  import { pluginManager } from '$src/lib/plugins/pluginManager.svelte';
  import type { PluginRecord, WidgetPlaybackSnapshot } from '$src/lib/plugins/types';
  import { playerStore } from '$src/state/player.svelte';

  let frames = $state<Record<string, HTMLIFrameElement | null>>({});

  let snapshot = $derived<WidgetPlaybackSnapshot>({
    currentTrack: playerStore.currentTrack
      ? {
          id: playerStore.currentTrack.id,
          title: playerStore.currentTrack.title,
          artist: playerStore.currentTrack.artist,
          album: playerStore.currentTrack.album,
          artworkUrl: playerStore.currentTrack.artworkUrl,
          accentColor: playerStore.currentTrack.accentColor,
          codec: playerStore.currentTrack.codec,
          bitDepth: playerStore.currentTrack.bitDepth,
          sampleRateHz: playerStore.currentTrack.sampleRateHz,
          durationSec: playerStore.currentTrack.durationSec,
        }
      : null,
    isPlaying: playerStore.isPlaying,
    positionMs: Math.round(playerStore.positionSec * 1000),
    durationMs: Math.round(playerStore.durationSec * 1000),
  });

  function buildSrcDoc(widget: PluginRecord): string {
    const closeTag = '</' + 'script>';
    const openTag = '<' + 'script>';
    const safeCode = (widget.sourceCode || '').replace(new RegExp(closeTag, 'gi'), '<\\/' + 'script>');
    return `<!doctype html>
<html>
<head>
<meta charset="utf-8">
<style>
  html, body { margin: 0; padding: 0; background: transparent; overflow: hidden; user-select: none; }
</style>
</head>
<body>
<div id="lw-widget-root"></div>
${openTag}
(function () {
  var listeners = { trackChange: new Set(), progress: new Set() };
  var lastTrackId = null;
  var state = {
    currentTrack: null,
    isPlaying: false,
    positionMs: 0,
    durationMs: 0
  };
  Object.defineProperty(window, 'LastWave', {
    value: Object.freeze({
      get currentTrack() { return state.currentTrack; },
      get isPlaying() { return state.isPlaying; },
      get positionMs() { return state.positionMs; },
      get durationMs() { return state.durationMs; },
      on: function (ev, fn) {
        if (listeners[ev] && typeof fn === 'function') listeners[ev].add(fn);
        return function () { listeners[ev] && listeners[ev].delete(fn); };
      },
      off: function (ev, fn) {
        if (listeners[ev]) listeners[ev].delete(fn);
      }
    }),
    writable: false,
    configurable: false
  });
  window.addEventListener('message', function (e) {
    if (!e.data || e.data.type !== 'lw:snapshot' || !e.data.payload) return;
    var p = e.data.payload;
    var nextId = p.currentTrack ? p.currentTrack.id : null;
    var trackChanged = nextId !== lastTrackId;
    lastTrackId = nextId;
    state.currentTrack = p.currentTrack;
    state.isPlaying = Boolean(p.isPlaying);
    state.positionMs = Number(p.positionMs) || 0;
    state.durationMs = Number(p.durationMs) || 0;
    if (trackChanged) {
      listeners.trackChange.forEach(function (fn) { try { fn(state); } catch (_) {} });
    }
    listeners.progress.forEach(function (fn) { try { fn(state); } catch (_) {} });
  });
})();
${closeTag}
${openTag}
${safeCode}
${closeTag}
</body>
</html>`;
  }

  function pushSnapshot(el: HTMLIFrameElement | null) {
    el?.contentWindow?.postMessage({ type: 'lw:snapshot', payload: snapshot }, '*');
  }

  $effect(() => {
    void snapshot;
    for (const widget of pluginManager.activeWidgets) {
      pushSnapshot(frames[widget.manifest.id] ?? null);
    }
  });
</script>

{#if pluginManager.activeWidgets.length > 0}
  <div
    data-lw-slot="widgets"
    class="grid grid-cols-1 gap-2.5 border-t border-[var(--lw-divider)] bg-[var(--lw-bg-deep)]/60 px-4 py-2.5 md:grid-cols-2 lg:grid-cols-3"
  >
    {#each pluginManager.activeWidgets as widget (widget.manifest.id)}
      <iframe
        bind:this={frames[widget.manifest.id]}
        title={widget.manifest.name}
        sandbox="allow-scripts"
        srcdoc={buildSrcDoc(widget)}
        onload={() => pushSnapshot(frames[widget.manifest.id] ?? null)}
        class="h-[74px] w-full rounded-[14px] border-0 bg-transparent"
      ></iframe>
    {/each}
  </div>
{/if}
