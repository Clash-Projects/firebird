<script lang="ts">
  import { onMount } from 'svelte';
  import { ipcInvoke, isTauriRuntime } from '$src/lib/ipc';
  import { featuresStore } from '$src/state/features.svelte';
  import { libraryStore } from '$src/state/library.svelte';
  import { settingsStore } from '$src/state/settings.svelte';
  import Buttons from '$src/ui/components/Buttons.svelte';
  import Switch from '$src/ui/components/Switch.svelte';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';

  interface ExclusionEntry {
    key: string;
    name: string;
    artist: string;
  }

  let exclusions = $state<ExclusionEntry[]>([]);
  let syncing = $state(false);

  async function loadExclusions() {
    if (!isTauriRuntime()) return;
    const list = await ipcInvoke<ExclusionEntry[]>('library_exclusions_list').catch(() => []);
    if (Array.isArray(list)) exclusions = list;
  }

  onMount(() => {
    void loadExclusions();
  });

  async function restoreExclusion(key: string) {
    if (isTauriRuntime()) {
      await ipcInvoke('library_exclusions_remove', { key }).catch(() => null);
      void libraryStore.loadHomeFeed(true);
    }
    exclusions = exclusions.filter((e) => e.key !== key);
  }

  async function syncNow() {
    if (syncing || !settingsStore.syncYtLibrary || !isTauriRuntime()) return;
    syncing = true;
    try {
      await ipcInvoke('yt_library_sync_kick');
      await libraryStore.loadLocalStorage();
      featuresStore.showToast({ title: 'YouTube Music sync started', severity: 'info' });
    } finally {
      setTimeout(() => {
        syncing = false;
      }, 900);
    }
  }
</script>

<div class="space-y-5">
  <!-- Podcasts -->
  <div>
    <div class="text-[15px] font-bold text-[var(--lw-text-primary)]">Podcasts</div>
    <div class="mt-1.5 flex items-center justify-between gap-3">
      <p class="text-[12px] leading-relaxed text-[var(--lw-text-secondary)]">
        Show YouTube Music podcasts on Home, Search, and the Podcasts page. Turning this off keeps saved resume positions.
      </p>
      <Switch
        label="Enable podcasts"
        checked={settingsStore.podcastsEnabled}
        onchange={(v) => {
          settingsStore.podcastsEnabled = v;
          settingsStore.saveBoolPref('lw_podcasts_enabled', v);
        }}
      />
    </div>
  </div>

  <!-- YouTube Music sync -->
  <div>
    <div class="text-[15px] font-bold text-[var(--lw-text-primary)]">YouTube Music sync</div>
    <div class="mt-1.5 flex items-center justify-between gap-3">
      <p class="text-[12px] leading-relaxed text-[var(--lw-text-secondary)]">
        Keep Liked Songs and your playlists the same here and on YouTube Music. Changes here go up within seconds; changes on YouTube arrive within about a minute. Local songs are never uploaded, and new playlists are created as private.
      </p>
      <Switch
        label="YouTube Music sync"
        checked={settingsStore.syncYtLibrary}
        onchange={(v) => {
          settingsStore.syncYtLibrary = v;
          settingsStore.saveBoolPref('lw_sync_yt_library', v);
          if (v) void syncNow();
        }}
      />
    </div>
    <div class="mt-2.5 flex flex-wrap items-center gap-2">
      <Buttons
        variant="secondary"
        size="sm"
        disabled={syncing || !settingsStore.syncYtLibrary}
        onclick={() => void syncNow()}
      >
        <WaveIcon name="refresh" size={14} class={syncing ? 'animate-spin' : ''} />
        {syncing ? 'Syncing…' : 'Sync now'}
      </Buttons>
    </div>
  </div>

  <!-- Hidden from recommendations -->
  <div>
    <div class="text-[15px] font-bold text-[var(--lw-text-primary)]">Hidden from recommendations</div>
    <p class="mt-1 text-[12px] text-[var(--lw-text-secondary)]">
      Restore a track to let Mix Lab and Home suggest it again.
    </p>
    <div class="mt-2 space-y-1.5">
      {#if exclusions.length === 0}
        <p class="text-[12px] text-[var(--lw-text-tertiary)]">No tracks are hidden.</p>
      {:else}
        {#each exclusions as item (item.key)}
          <div class="flex items-center justify-between gap-3 py-1">
            <div class="min-w-0 flex-1">
              <div class="truncate text-[13.5px] font-semibold text-[var(--lw-text-primary)]">
                {item.name || item.key}
              </div>
              {#if item.artist}
                <div class="truncate text-[12px] text-[var(--lw-text-secondary)]">{item.artist}</div>
              {/if}
            </div>
            <Buttons variant="secondary" size="sm" onclick={() => void restoreExclusion(item.key)}>
              Restore
            </Buttons>
          </div>
        {/each}
      {/if}
    </div>
  </div>
</div>
