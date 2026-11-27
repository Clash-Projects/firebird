<script lang="ts">
  import { windowState } from '$src/app/window.svelte';
  import { featuresStore } from '$src/state/features.svelte';
  import { playerStore } from '$src/state/player.svelte';
  import { settingsStore } from '$src/state/settings.svelte';
  import WaveHaze from '$src/ui/theme/WaveHaze.svelte';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';

  let track = $derived(playerStore.currentTrack);
  let device = $derived(featuresStore.selectedDevice);
  let bitPerfectActive = $derived(
    settingsStore.exclusiveMode &&
      settingsStore.bitPerfectEnabled &&
      device.supportsExclusive &&
      !featuresStore.eqEnabled
  );
  let blockingReason = $derived(
    !settingsStore.exclusiveMode
      ? 'WASAPI Exclusive mode is off'
      : !device.supportsExclusive
        ? 'Selected device does not support Exclusive mode'
        : featuresStore.eqEnabled
          ? 'Parametric EQ DSP is active'
          : null
  );
</script>

<WaveHaze
  level="l2"
  class="flex h-full w-[360px] shrink-0 flex-col border-l border-[var(--lw-divider)] bg-[var(--lw-panel-translucent)] p-4 shadow-[var(--lw-shadow-panel)]"
>
  <div class="flex items-center justify-between pb-3">
    <h2 class="text-[14px] font-bold">Signal Path & Output</h2>
    <button
      type="button"
      onclick={() => windowState.closeRightPanel()}
      class="flex h-7 w-7 cursor-pointer items-center justify-center rounded-[8px] text-[var(--lw-text-secondary)] hover:bg-[var(--lw-wash-hover)]"
    >
      <WaveIcon name="close" size={15} />
    </button>
  </div>

  <div class="lw-scroll flex-1 space-y-4">
    <div class="rounded-[16px] border border-[var(--lw-outline)] bg-[var(--lw-surface)] p-3.5">
      <div class="flex items-center justify-between">
        <span class="text-[12px] font-semibold">Bit-Perfect Status</span>
        <span
          class="rounded-full px-2 py-0.5 text-[11px] font-semibold {bitPerfectActive
            ? 'bg-[var(--lw-success)]/15 text-[var(--lw-success)]'
            : 'bg-[var(--lw-warn)]/15 text-[var(--lw-warn)]'}"
        >
          {bitPerfectActive ? 'Bit-Perfect' : 'Shared / DSP'}
        </span>
      </div>
      {#if blockingReason}
        <p class="mt-1.5 text-[11.5px] text-[var(--lw-text-secondary)]">{blockingReason}</p>
      {/if}
      {#if track}
        <div class="mt-3 space-y-1 border-t border-[var(--lw-divider)] pt-2.5 text-[12px] text-[var(--lw-text-secondary)]">
          <div class="flex justify-between"><span>Source</span><span class="text-[var(--lw-text-primary)]">{track.codec}</span></div>
          <div class="flex justify-between"><span>Sample Rate</span><span class="lw-tabular text-[var(--lw-text-primary)]">{(track.sampleRateHz / 1000).toFixed(1)} kHz / {track.bitDepth}-bit</span></div>
          <div class="flex justify-between"><span>Output Driver</span><span class="text-[var(--lw-text-primary)]">{device.driver} {settingsStore.exclusiveMode ? 'Exclusive' : 'Shared'}</span></div>
        </div>
      {/if}
    </div>

    <div class="space-y-2">
      <div class="text-[11px] font-bold tracking-[0.8px] text-[var(--lw-text-tertiary)] uppercase">Output Device</div>
      {#each featuresStore.devices as dev (dev.id)}
        {@const selected = dev.id === featuresStore.selectedDeviceId}
        <button
          type="button"
          onclick={() => (featuresStore.selectedDeviceId = dev.id)}
          class="flex w-full cursor-pointer items-center justify-between rounded-[12px] border px-3 py-2.5 text-left transition-colors {selected
            ? 'border-[var(--lw-accent)] bg-[var(--lw-wash-pressed)]'
            : 'border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] hover:bg-[var(--lw-wash-hover)]'}"
        >
          <div class="min-w-0">
            <div class="truncate text-[12.5px] font-semibold">{dev.name}</div>
            <div class="text-[11px] text-[var(--lw-text-tertiary)]">
              {dev.driver} · up to {dev.maxBitDepth}-bit / {(dev.maxSampleRateHz / 1000).toFixed(0)} kHz
            </div>
          </div>
          {#if selected}
            <WaveIcon name="check" size={15} class="text-[var(--lw-accent)]" />
          {/if}
        </button>
      {/each}
    </div>

    <label class="flex cursor-pointer items-center justify-between rounded-[12px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] px-3 py-2.5">
      <div>
        <div class="text-[12.5px] font-semibold">WASAPI Exclusive Mode</div>
        <div class="text-[11px] text-[var(--lw-text-tertiary)]">Bypass Windows mixer for direct DAC output</div>
      </div>
      <input type="checkbox" bind:checked={settingsStore.exclusiveMode} class="h-4 w-4 accent-[var(--lw-accent)]" />
    </label>
  </div>
</WaveHaze>
