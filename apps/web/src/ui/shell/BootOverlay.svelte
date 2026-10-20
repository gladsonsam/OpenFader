<!-- Boot overlay: covers the shell while the engine link is being
     established (live.phase === 'connecting'). Same visual language as the
     iOS LaunchScreen and the index.html static placeholder, so launch is one
     continuous dark surface: storyboard -> static -> this -> live mixer.
     Status line is the live link message itself ('Connecting to embedded
     engine …'), so it narrates real progress instead of a dead spinner.
     Never renders in fixture/offline/gallery modes: those never enter
     'connecting', and this component is only mounted by Shell. -->
<script lang="ts">
  import { fade } from 'svelte/transition';
  import { REDUCED_MOTION } from '../primitives/feel.ts';

  let { message }: { message: string } = $props();
</script>

<div class="boot" transition:fade={{ duration: REDUCED_MOTION ? 0 : 180 }}>
  <div class="word">openmix</div>
  <div class="spin" aria-hidden="true"></div>
  <div class="msg">{message}</div>
</div>

<style>
  .boot {
    position: fixed;
    inset: 0;
    z-index: 90;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 22px;
    background: #0b0e14;
  }
  .word {
    color: #fff;
    font: 700 40px/1 -apple-system, system-ui, sans-serif;
  }
  .spin {
    width: 34px;
    height: 34px;
    border-radius: 50%;
    border: 3px solid rgba(255, 255, 255, 0.18);
    border-top-color: #fff;
    animation: bootspin 0.9s linear infinite;
  }
  .msg {
    color: rgba(255, 255, 255, 0.55);
    font: 13px/1.4 -apple-system, system-ui, sans-serif;
    letter-spacing: 2px;
    text-transform: uppercase;
  }
  @keyframes bootspin {
    to {
      transform: rotate(360deg);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .spin {
      animation: none;
    }
  }
</style>
