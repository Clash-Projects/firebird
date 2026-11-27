<script lang="ts">
  import type { Snippet } from 'svelte';
  import { windowState } from '$src/app/window.svelte';
  import WelcomePage from '$src/ui/auth/WelcomePage.svelte';

  interface Props {
    children: Snippet;
  }

  let { children }: Props = $props();
</script>

{#if windowState.welcomeOpen}
  <WelcomePage onComplete={() => (windowState.welcomeOpen = false)} />
{:else}
  {@render children()}
{/if}
