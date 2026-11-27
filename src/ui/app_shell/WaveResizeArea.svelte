<script lang="ts">
  import { onMount, type Snippet } from 'svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { windowState } from '$src/app/window.svelte';
  import { isTauriRuntime } from '$src/lib/ipc';

  interface Props {
    children: Snippet;
  }

  let { children }: Props = $props();
  const appWindow = isTauriRuntime() ? getCurrentWindow() : null;

  type ResizeDir =
    | 'North'
    | 'South'
    | 'East'
    | 'West'
    | 'NorthEast'
    | 'NorthWest'
    | 'SouthEast'
    | 'SouthWest';

  function startResize(e: PointerEvent, dir: ResizeDir) {
    if (!appWindow || e.button !== 0 || windowState.isMaximized || windowState.isFullscreen) return;
    e.preventDefault();
    void appWindow.startResizeDragging(dir);
  }

  onMount(() => {
    if (!appWindow) return;
    let unlisten: (() => void) | undefined;
    const syncMax = async () => {
      try {
        windowState.isMaximized = await appWindow.isMaximized();
      } catch {
        // Ignore if unavailable
      }
    };
    void syncMax();
    void appWindow.onResized(() => {
      void syncMax();
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  });

  let handlesActive = $derived(
    Boolean(appWindow) && !windowState.isMaximized && !windowState.isFullscreen
  );
</script>

<div class="relative flex h-full w-full flex-col overflow-hidden">
  {@render children()}

  {#if handlesActive}
    <!-- 4-edge & 4-corner native OS resize grips for frameless window (matches wave_resize_area.dart) -->
    <div
      role="presentation"
      class="fixed top-0 left-[14px] right-[14px] z-[90] h-[4px] cursor-ns-resize"
      onpointerdown={(e) => startResize(e, 'North')}
    ></div>
    <div
      role="presentation"
      class="fixed bottom-0 left-[14px] right-[14px] z-[90] h-[4px] cursor-ns-resize"
      onpointerdown={(e) => startResize(e, 'South')}
    ></div>
    <div
      role="presentation"
      class="fixed top-[14px] bottom-[14px] left-0 z-[90] w-[4px] cursor-ew-resize"
      onpointerdown={(e) => startResize(e, 'West')}
    ></div>
    <div
      role="presentation"
      class="fixed top-[14px] bottom-[14px] right-0 z-[90] w-[4px] cursor-ew-resize"
      onpointerdown={(e) => startResize(e, 'East')}
    ></div>

    <div
      role="presentation"
      class="fixed top-0 left-0 z-[95] h-[14px] w-[14px] cursor-nwse-resize"
      onpointerdown={(e) => startResize(e, 'NorthWest')}
    ></div>
    <div
      role="presentation"
      class="fixed top-0 right-0 z-[95] h-[14px] w-[14px] cursor-nesw-resize"
      onpointerdown={(e) => startResize(e, 'NorthEast')}
    ></div>
    <div
      role="presentation"
      class="fixed bottom-0 left-0 z-[95] h-[14px] w-[14px] cursor-nesw-resize"
      onpointerdown={(e) => startResize(e, 'SouthWest')}
    ></div>
    <div
      role="presentation"
      class="fixed right-0 bottom-0 z-[95] h-[14px] w-[14px] cursor-nwse-resize"
      onpointerdown={(e) => startResize(e, 'SouthEast')}
    ></div>
  {/if}
</div>
