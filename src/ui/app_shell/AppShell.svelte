<script lang="ts">
  import { router } from '$src/app/router.svelte';
  import { windowState } from '$src/app/window.svelte';
  import OutputPathSheet from '$src/features/audio_output/OutputPathSheet.svelte';
  import EqualizerPanel from '$src/features/equalizer/EqualizerPanel.svelte';
  import { uiSlots } from '$src/lib/plugins/uiSlots.svelte';
  import WidgetSandboxHost from '$src/lib/plugins/WidgetSandboxHost.svelte';
  import { settingsStore } from '$src/state/settings.svelte';
  import AlbumDetailPage from '$src/ui/collections/AlbumDetailPage.svelte';
  import AlbumsPage from '$src/ui/collections/AlbumsPage.svelte';
  import ArtistDetailPage from '$src/ui/collections/ArtistDetailPage.svelte';
  import ArtistsPage from '$src/ui/collections/ArtistsPage.svelte';
  import DownloadsPage from '$src/ui/collections/DownloadsPage.svelte';
  import GenreDetailPage from '$src/ui/collections/GenreDetailPage.svelte';
  import HistoryPage from '$src/ui/collections/HistoryPage.svelte';
  import LikedPage from '$src/ui/collections/LikedPage.svelte';
  import PlaylistDetailPage from '$src/ui/collections/PlaylistDetailPage.svelte';
  import PlaylistsPage from '$src/ui/collections/PlaylistsPage.svelte';
  import SpotifyPlaylistDetailPage from '$src/ui/collections/SpotifyPlaylistDetailPage.svelte';
  import YtPlaylistDetailPage from '$src/ui/collections/YtPlaylistDetailPage.svelte';
  import YtShelfPage from '$src/ui/collections/YtShelfPage.svelte';
  import ContextMenu from '$src/ui/components/ContextMenu.svelte';
  import GlassStage from '$src/ui/components/GlassStage.svelte';
  import InfobarHost from '$src/ui/components/InfobarHost.svelte';
  import GenresPage from '$src/ui/genres/GenresPage.svelte';
  import HomeLayoutPage from '$src/ui/home/HomeLayoutPage.svelte';
  import HomePage from '$src/ui/home/HomePage.svelte';
  import JamPage from '$src/ui/jam/JamPage.svelte';
  import LibraryPage from '$src/ui/library/LibraryPage.svelte';
  import LyricsPanel from '$src/ui/lyrics/LyricsPanel.svelte';
  import MiniPlayer from '$src/ui/mini_player/MiniPlayer.svelte';
  import SupportPages from '$src/ui/misc/SupportPages.svelte';
  import SideRail from '$src/ui/navigation/SideRail.svelte';
  import NowPlayingPage from '$src/ui/now_playing/NowPlayingPage.svelte';
  import { immersiveState } from '$src/ui/now_playing/immersive.svelte';
  import PlayerDock from '$src/ui/player_dock/PlayerDock.svelte';
  import PodcastsPage from '$src/ui/podcasts/PodcastsPage.svelte';
  import QueuePanel from '$src/ui/queue/QueuePanel.svelte';
  import SearchPage from '$src/ui/search/SearchPage.svelte';
  import SettingsPage from '$src/ui/settings/SettingsPage.svelte';
  import StatsPage from '$src/ui/stats/StatsPage.svelte';
  import StorePage from '$src/ui/store/StorePage.svelte';
  import UpdateDialog from '$src/ui/updates/UpdateDialog.svelte';
  import BranchSwitcher from './BranchSwitcher.svelte';
  import CommandPalette from './CommandPalette.svelte';
  import CustomAppHost from './CustomAppHost.svelte';
  import TitleBar from './TitleBar.svelte';
  import WaveResizeArea from './WaveResizeArea.svelte';
  import { WaveDensity } from '$src/ui/theme/tokens';
  import { waveSlide } from '$src/ui/theme/motion';

  let customApp = $derived(uiSlots.customAppRoutes.find((a) => a.route === router.path) ?? null);
  let isNowPlaying = $derived(router.path === '/now-playing');
  let isImmersive = $derived(isNowPlaying && immersiveState.isImmersive);
  let floatDock = $derived(
    !settingsStore.performanceMode && settingsStore.glassmorphism && !isImmersive
  );
  let openPanels = $derived(
    windowState.activeRightPanel === 'none' ? [] : [windowState.activeRightPanel]
  );
</script>

<GlassStage />

{#if windowState.isMiniPlayer}
  <MiniPlayer />
{:else}
  <WaveResizeArea>
    <div
      data-lw="app-shell"
      class="flex h-full w-full flex-col bg-[var(--lw-bg)] text-[var(--lw-text-primary)]"
      style:--lw-shell-rail-width="{windowState.railExpanded ? WaveDensity.railExpanded : WaveDensity.railCollapsed}px"
    >
      {#if !isImmersive}
        {#if uiSlots.titleBarOverride}
          <div data-lw="title-bar" class="h-[var(--lw-titlebar-h,44px)] w-full flex-none">
            <CustomAppHost app={uiSlots.titleBarOverride} />
          </div>
        {:else}
          <TitleBar />
        {/if}
      {/if}

      <div class="relative flex min-h-0 flex-1 overflow-hidden">
        <!-- SideRail stays visible on Now Playing unless full-screen immersive (matches app_shell.dart) -->
        {#if !isImmersive}
          {#if uiSlots.sideRailOverride}
            <div data-lw="sidebar" class="h-full flex-none">
              <CustomAppHost app={uiSlots.sideRailOverride} />
            </div>
          {:else}
            <SideRail />
          {/if}
        {/if}

        <main data-lw="main-stage" class="relative min-w-0 flex-1 overflow-hidden">
          <BranchSwitcher routeKey={router.path} hasDockSpacer={floatDock}>
            {#if customApp}
              <CustomAppHost app={customApp} />
            {:else if router.path === '/now-playing'}
              {#if uiSlots.nowPlayingOverride}
                <CustomAppHost app={uiSlots.nowPlayingOverride} />
              {:else}
                <NowPlayingPage />
              {/if}
            {:else if router.path === '/home/layout'}
              <HomeLayoutPage />
            {:else if router.matches('/search')}
              <SearchPage initialQuery={decodeURIComponent(router.segment(1))} />
            {:else if router.path === '/genres'}
              <GenresPage />
            {:else if router.matches('/genre')}
              <GenreDetailPage genreId={router.segment(1)} />
            {:else if router.path === '/podcasts'}
              <PodcastsPage />
            {:else if router.path === '/library'}
              <LibraryPage />
            {:else if router.path === '/playlists'}
              <PlaylistsPage />
            {:else if router.matches('/playlist')}
              <PlaylistDetailPage playlistId={router.segment(1)} />
            {:else if router.matches('/yt-playlist')}
              <YtPlaylistDetailPage playlistId={router.segment(1)} />
            {:else if router.matches('/spotify-playlist')}
              <SpotifyPlaylistDetailPage playlistId={router.segment(1)} />
            {:else if router.path === '/yt-shelf'}
              <YtShelfPage />
            {:else if router.path === '/liked'}
              <LikedPage />
            {:else if router.path === '/albums'}
              <AlbumsPage />
            {:else if router.matches('/album')}
              <AlbumDetailPage albumId={router.segment(1)} />
            {:else if router.path === '/artists'}
              <ArtistsPage />
            {:else if router.matches('/artist')}
              <ArtistDetailPage artistId={router.segment(1)} />
            {:else if router.path === '/downloads'}
              <DownloadsPage />
            {:else if router.path === '/history'}
              <HistoryPage />
            {:else if router.path === '/jam'}
              <JamPage />
            {:else if router.path === '/store'}
              <StorePage />
            {:else if router.path === '/stats'}
              <StatsPage />
            {:else if router.matches('/settings')}
              <SettingsPage initialSection={router.segment(1) || 'general'} />
            {:else if router.path === '/support'}
              <SupportPages />
            {:else}
              <HomePage />
            {/if}
          </BranchSwitcher>

          <!-- Overlay drawers slide over content without squeezing main-stage width -->
          {#if openPanels.length > 0}
            <div class="shell-panels" style:bottom={floatDock ? 'var(--lw-dock-h, 68px)' : '0'}>
              {#each openPanels as panel (panel)}
                <div class="shell-panel" transition:waveSlide={{ x: 24 }}>
                  {#if panel === 'queue'}
                    <QueuePanel />
                  {:else if panel === 'lyrics'}
                    <LyricsPanel />
                  {:else if panel === 'eq'}
                    <EqualizerPanel />
                  {:else if panel === 'output'}
                    <OutputPathSheet />
                  {/if}
                </div>
              {/each}
            </div>
          {/if}

          {#if floatDock}
            <div class="pointer-events-auto absolute inset-x-0 bottom-0 z-20">
              {#if uiSlots.playerBarOverride}
                <div data-lw="player-bar" class="h-[var(--lw-dock-h,68px)] w-full flex-none">
                  <CustomAppHost app={uiSlots.playerBarOverride} />
                </div>
              {:else}
                <PlayerDock />
              {/if}
            </div>
          {/if}
        </main>
      </div>

      <WidgetSandboxHost />
      {#if !floatDock && !isImmersive}
        {#if uiSlots.playerBarOverride}
          <div data-lw="player-bar" class="h-[var(--lw-dock-h,68px)] w-full flex-none">
            <CustomAppHost app={uiSlots.playerBarOverride} />
          </div>
        {:else}
          <PlayerDock />
        {/if}
      {/if}
    </div>
  </WaveResizeArea>
{/if}

<CommandPalette />
<ContextMenu />
<InfobarHost />
<UpdateDialog />

<style>
  .shell-panels {
    position: absolute;
    top: 0;
    right: 0;
    z-index: 25;
    display: grid;
    grid-template-rows: minmax(0, 1fr);
    max-width: 100%;
  }

  .shell-panel {
    grid-area: 1 / 1;
    min-height: 0;
    height: 100%;
  }

  .shell-panel :global(.lw-scroll) {
    overscroll-behavior: contain;
  }
</style>
