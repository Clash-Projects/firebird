<script lang="ts">
  import { dismissDialogOnBackdrop } from '$src/app/accessibility';
  import { featuresStore } from '$src/state/features.svelte';
  import { libraryStore } from '$src/state/library.svelte';
  import type { Playlist, Track } from '$src/state/types';
  import Artwork from '$src/ui/components/Artwork.svelte';
  import Buttons from '$src/ui/components/Buttons.svelte';
  import WaveIcon from '$src/ui/theme/WaveIcon.svelte';

  interface Props {
    open: boolean;
    track?: Track | null;
    onClose: () => void;
    onCreated?: (id: string) => void;
  }

  let { open, track = null, onClose, onCreated }: Props = $props();
  const id = $props.id();
  let dialogEl = $state<HTMLDialogElement | null>(null);
  let name = $state('');
  let description = $state('');
  let saving = $state(false);
  let error = $state('');
  let createdPlaylist = $state<Playlist | null>(null);
  let action = $derived(createdPlaylist ? 'Retry adding song' : track ? 'Create and add' : 'Create playlist');

  $effect(() => {
    if (!dialogEl) return;
    if (open && !dialogEl.open) {
      name = '';
      description = '';
      error = '';
      createdPlaylist = null;
      dialogEl.showModal();
    } else if (!open && dialogEl.open) {
      dialogEl.close();
    }
  });

  function close() {
    if (!saving) onClose();
  }

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    const song = track;
    if (saving || (song && !name.trim())) return;

    saving = true;
    error = '';
    try {
      const playlist = createdPlaylist ?? await libraryStore.createPlaylist(name, description);
      if (!playlist) {
        error = 'Could not create the playlist. Try again.';
        return;
      }

      if (song) {
        createdPlaylist = playlist;
        if (!await libraryStore.addTrackToPlaylist(playlist.id, song)) {
          error = 'The playlist was created. Try adding the song again.';
          return;
        }
        featuresStore.showToast({
          title: `Added to ${playlist.name}`,
          subtitle: song.title,
          severity: 'success',
        });
      }

      if (open) {
        onClose();
        onCreated?.(playlist.id);
      }
    } catch {
      error = createdPlaylist
        ? 'The playlist was created. Try adding the song again.'
        : 'Could not create the playlist. Try again.';
    } finally {
      saving = false;
    }
  }
</script>

<dialog
  bind:this={dialogEl}
  closedby={saving ? 'none' : 'any'}
  use:dismissDialogOnBackdrop={close}
  oncancel={(event) => { if (saving) event.preventDefault(); }}
  onclose={() => { if (open && !dialogEl?.open) onClose(); }}
  aria-labelledby={id + '-title'}
  class="new-playlist-dialog"
>
  <form onsubmit={submit} aria-busy={saving}>
    <div class="playlist-dialog-body">
      <header class="playlist-dialog-heading">
        <span class="playlist-dialog-icon" aria-hidden="true"><WaveIcon name="playlists" size={23} /></span>
        <h2 id={id + '-title'}>New playlist</h2>
      </header>

      {#if track}
        <div class="playlist-dialog-track">
          <Artwork src={track.artworkUrl} kind="track" upgrade={false} size={44} radius={6} />
          <div class="playlist-dialog-track-text">
            <span class="playlist-dialog-track-caption">Song to add</span>
            <strong title={track.title}>{track.title}</strong>
            {#if track.artist}<span title={track.artist}>{track.artist}</span>{/if}
          </div>
        </div>
      {/if}

      <div class="playlist-fields">
        <div class="playlist-field">
          <label for={id + '-name'}>Name</label>
          <!-- svelte-ignore a11y_autofocus -->
          <input
            id={id + '-name'}
            class="lw-text-field"
            bind:value={name}
            placeholder="Playlist name"
            disabled={saving || Boolean(createdPlaylist)}
            autofocus
          />
        </div>
        {#if !track}
          <div class="playlist-field">
            <label for={id + '-description'}>Description <span>Optional</span></label>
            <input
              id={id + '-description'}
              class="lw-text-field"
              bind:value={description}
              placeholder="What’s this playlist for?"
              disabled={saving}
            />
          </div>
        {/if}
      </div>

      {#if error}<p class="playlist-dialog-error" role="alert">{error}</p>{/if}
    </div>

    <div class="playlist-dialog-actions">
      <Buttons variant="ghost" onclick={close} disabled={saving}>Cancel</Buttons>
      <Buttons type="submit" variant="primary" disabled={saving || Boolean(track && !name.trim())}>
        {saving ? 'Saving…' : action}
      </Buttons>
    </div>
  </form>
</dialog>

<style>
  .new-playlist-dialog {
    width: min(440px, calc(100vw - 32px));
  }

  .playlist-dialog-body {
    padding: 24px clamp(18px, 5vw, 26px) 22px;
  }

  .playlist-dialog-heading {
    display: flex;
    align-items: center;
    gap: 14px;
    margin-bottom: 20px;
  }

  .playlist-dialog-icon {
    display: grid;
    flex: none;
    width: 44px;
    height: 44px;
    place-items: center;
    border: 1px solid var(--lw-outline-soft);
    border-radius: 10px;
    background: color-mix(in srgb, #abc17e 17%, var(--lw-surface-raised));
    color: var(--lw-text-primary);
  }

  .playlist-dialog-heading h2 {
    font-size: 25px;
    font-weight: 750;
    line-height: 1.12;
    letter-spacing: -0.04em;
  }

  .playlist-dialog-track {
    display: flex;
    align-items: center;
    gap: 11px;
    padding: 9px 10px;
    margin-bottom: 18px;
    border-radius: 10px;
    background: var(--lw-wash-hover);
  }

  .playlist-dialog-track-text {
    min-width: 0;
    color: var(--lw-text-secondary);
    font-size: 12px;
    line-height: 1.4;
  }

  .playlist-dialog-track-text > * {
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .playlist-dialog-track-text strong {
    color: var(--lw-text-primary);
    font-size: 13px;
    font-weight: 650;
  }

  .playlist-dialog-track-caption {
    font-size: 11px;
  }

  .playlist-fields {
    display: grid;
    gap: 14px;
  }

  .playlist-field {
    display: grid;
    gap: 6px;
  }

  .playlist-field label {
    font-size: 13px;
    font-weight: 650;
  }

  .playlist-field label span {
    margin-inline-start: 5px;
    color: var(--lw-text-secondary);
    font-size: 11px;
    font-weight: 400;
  }

  .playlist-field input {
    min-width: 0;
    width: 100%;
    height: 40px;
    padding-inline: 12px;
    border: 1px solid var(--lw-outline);
    border-radius: 8px;
    background: var(--lw-surface);
    font-size: 14px;
  }

  .playlist-field input::placeholder {
    color: var(--lw-text-tertiary);
  }

  .playlist-field input:focus {
    border-color: color-mix(in srgb, var(--lw-text-primary) 52%, var(--lw-outline));
    background: var(--lw-surface);
    box-shadow: none;
  }

  @media (forced-colors: active) {
    .playlist-field input:focus {
      border-color: Highlight;
    }
  }

  .playlist-dialog-error {
    margin-top: 12px;
    color: var(--lw-danger);
    font-size: 12px;
    line-height: 1.45;
  }

  .playlist-dialog-actions {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 8px;
    padding: 14px clamp(18px, 5vw, 26px);
    border-top: 1px solid var(--lw-divider);
    background: color-mix(in srgb, var(--lw-surface) 40%, var(--lw-surface-raised));
  }
</style>
