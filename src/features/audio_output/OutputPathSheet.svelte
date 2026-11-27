<script lang="ts">
  import Switch from '$src/ui/components/Switch.svelte';
  import { waveTooltip } from '$src/ui/components/tooltip';
  import { waveSmoothScroll } from '$src/ui/theme/smoothScroll';
  import { windowState } from '$src/app/window.svelte';
  import { getPlatformAudioCopy } from '$src/features/audio_output/platformAudio';
  import { ipcInvoke } from '$src/lib/ipc';
  import { featuresStore } from '$src/state/features.svelte';
  import { playerStore } from '$src/state/player.svelte';
  import { settingsStore } from '$src/state/settings.svelte';
  import WaveHaze from '$src/ui/theme/WaveHaze.svelte';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';

  let track = $derived(playerStore.currentTrack);
  let device = $derived(featuresStore.selectedDevice);
  let audioCopy = $derived(getPlatformAudioCopy(settingsStore.platform));
  let bitPerfectActive = $derived(
    settingsStore.exclusiveMode &&
      settingsStore.bitPerfectEnabled &&
      (device ? device.supportsExclusive : true) &&
      !featuresStore.eqEnabled
  );
  let blockingReason = $derived(
    !settingsStore.bitPerfectEnabled || !settingsStore.exclusiveMode
      ? audioCopy.exclusiveOffReason
      : device && !device.supportsExclusive
        ? 'Selected device does not support exclusive output'
        : featuresStore.eqEnabled
          ? 'Parametric EQ DSP is active'
          : null
  );

  function toggleBitPerfect(enabled: boolean) {
    settingsStore.bitPerfectEnabled = enabled;
    settingsStore.exclusiveMode = enabled;
    void ipcInvoke('audio_output_set_bit_perfect', { enabled });
    void ipcInvoke('audio_output_set_exclusive', { exclusive: enabled });
  }
</script>

<WaveHaze
  level="l2"
  class="flex h-full w-[min(360px,100vw)] shrink-0 flex-col border-l border-[var(--lw-divider)] bg-[var(--lw-panel-translucent)] p-4 shadow-[var(--lw-shadow-panel)]"
>
  <div class="flex items-center justify-between pb-3">
    <h2 class="text-[14px] font-semibold">Signal Path &amp; Output</h2>
    <button
      type="button"
      use:waveTooltip={'Close panel'}
      onclick={() => windowState.closeRightPanel()}
      aria-label="Close audio output"
      class="flex h-7 w-7 cursor-pointer items-center justify-center rounded-[6px] text-[var(--lw-text-secondary)] hover:bg-[var(--lw-wash-hover)]"
    >
      <WaveIcon name="close" size={15} />
    </button>
  </div>

  <div use:waveSmoothScroll class="lw-scroll flex-1 space-y-4">
    <div class="rounded-[12px] border border-[var(--lw-outline)] bg-[var(--lw-surface)] p-3.5">
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
          <div class="flex justify-between"><span>Output Driver</span><span class="text-[var(--lw-text-primary)]">{device?.driver ?? audioCopy.driverName} {settingsStore.exclusiveMode ? 'Exclusive' : 'Shared'}</span></div>
        </div>
      {/if}
    </div>

    <div class="space-y-2">
      <div class="text-[11px] font-bold tracking-[0.8px] text-[var(--lw-text-tertiary)] uppercase">Output Device</div>
      {#if featuresStore.devices.length === 0}
        <div class="rounded-[12px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] px-3 py-3 text-[12px] text-[var(--lw-text-secondary)]">
          No audio devices available.
        </div>
      {:else}
        {#each featuresStore.devices as dev (dev.id)}
          {@const selected = dev.id === featuresStore.selectedDeviceId}
          <button
            type="button"
            onclick={() => featuresStore.selectDevice(dev.id)}
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
      {/if}
    </div>

    <label class="flex cursor-pointer items-center justify-between gap-3 rounded-[12px] border border-[var(--lw-outline-soft)] bg-[var(--lw-surface)] px-3 py-2.5">
      <div>
        <div class="text-[12.5px] font-semibold">{audioCopy.exclusiveTitle}</div>
        <div class="text-[11px] text-[var(--lw-text-tertiary)]">{audioCopy.exclusiveDescription}</div>
      </div>
      <Switch
        checked={settingsStore.bitPerfectEnabled && settingsStore.exclusiveMode}
        onchange={toggleBitPerfect}
      />
    </label>
  </div>
</WaveHaze>
