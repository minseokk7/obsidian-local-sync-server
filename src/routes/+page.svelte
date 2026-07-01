<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";

  let status = $state("Stopped");
  let port = $state(8080);
  let logs = $state<string[]>([]);

  async function toggleServer() {
    if (status === "Stopped") {
      try {
        await invoke("start_server", { port });
        status = "Running";
      } catch (err) {
        logs.push(`[ERROR] Failed to start server: ${err}`);
      }
    } else {
      try {
        await invoke("stop_server");
        status = "Stopped";
      } catch (err) {
        logs.push(`[ERROR] Failed to stop server: ${err}`);
      }
    }
  }

  onMount(() => {
    const unlisten = listen<string>("server-log", (event) => {
      logs = [...logs, event.payload];
    });

    return () => {
      unlisten.then(f => f());
    };
  });
</script>

<main class="min-h-screen p-8 flex flex-col items-center justify-center relative overflow-hidden bg-gradient-to-br from-neutral-900 via-neutral-800 to-black">
  <!-- Animated background blobs -->
  <div class="absolute top-1/4 left-1/4 w-96 h-96 bg-blue-600/30 rounded-full mix-blend-screen filter blur-[128px] animate-pulse"></div>
  <div class="absolute bottom-1/4 right-1/4 w-96 h-96 bg-emerald-600/30 rounded-full mix-blend-screen filter blur-[128px] animate-pulse" style="animation-delay: 2s;"></div>

  <div class="glass-dark w-full max-w-4xl rounded-3xl p-8 flex flex-col gap-8 relative z-10">
    <header class="flex justify-between items-center border-b border-white/10 pb-6">
      <div>
        <h1 class="text-3xl font-bold bg-gradient-to-r from-blue-400 to-emerald-400 bg-clip-text text-transparent">
          Obsidian Local Sync
        </h1>
        <p class="text-neutral-400 mt-2 text-sm">WebDAV sync server for remotely-save</p>
      </div>
      <div class="flex items-center gap-4">
        <div class="flex items-center gap-2 px-4 py-2 rounded-full glass-dark">
          <div class="w-3 h-3 rounded-full {status === 'Running' ? 'bg-emerald-500 animate-pulse' : 'bg-red-500'}"></div>
          <span class="text-sm font-medium">{status}</span>
        </div>
      </div>
    </header>

    <div class="grid grid-cols-1 md:grid-cols-3 gap-8">
      <div class="col-span-1 flex flex-col gap-6">
        <div class="flex flex-col gap-2">
          <label for="port" class="text-sm text-neutral-400 font-medium">Server Port</label>
          <input
            id="port"
            type="number"
            bind:value={port}
            disabled={status === 'Running'}
            class="w-full bg-black/50 border border-white/10 rounded-xl px-4 py-3 text-white focus:outline-none focus:ring-2 focus:ring-blue-500 transition-all disabled:opacity-50"
          />
        </div>

        <button
          onclick={toggleServer}
          class="w-full py-4 rounded-xl font-bold transition-all transform active:scale-95 flex items-center justify-center gap-2 {status === 'Running' ? 'bg-red-500/20 text-red-400 hover:bg-red-500/30 border border-red-500/50' : 'bg-blue-600 hover:bg-blue-500 text-white shadow-[0_0_20px_rgba(37,99,235,0.4)]'}"
        >
          {status === "Running" ? "Stop Server" : "Start Server"}
        </button>
      </div>

      <div class="col-span-1 md:col-span-2 glass-dark bg-black/60 rounded-xl p-4 flex flex-col h-64 border border-white/5">
        <h2 class="text-sm font-medium text-neutral-400 mb-3 flex items-center gap-2">
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 6h16M4 12h16M4 18h7"></path></svg>
          Server Logs
        </h2>
        <div class="flex-1 overflow-y-auto font-mono text-xs space-y-1 pr-2 custom-scrollbar">
          {#if logs.length === 0}
            <div class="text-neutral-600 italic mt-2">Waiting for server logs...</div>
          {:else}
            {#each logs as log}
              <div class="text-neutral-300 border-b border-white/5 pb-1">
                <span class="text-neutral-500">[{new Date().toLocaleTimeString()}]</span> {log}
              </div>
            {/each}
          {/if}
        </div>
      </div>
    </div>
  </div>
</main>

<style>
  .custom-scrollbar::-webkit-scrollbar {
    width: 6px;
  }
  .custom-scrollbar::-webkit-scrollbar-track {
    background: rgba(255, 255, 255, 0.02);
    border-radius: 4px;
  }
  .custom-scrollbar::-webkit-scrollbar-thumb {
    background: rgba(255, 255, 255, 0.1);
    border-radius: 4px;
  }
  .custom-scrollbar::-webkit-scrollbar-thumb:hover {
    background: rgba(255, 255, 255, 0.2);
  }
</style>
