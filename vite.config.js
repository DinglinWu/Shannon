import {defineConfig} from 'vite';
import {fileURLToPath} from 'node:url';
const mode=process.env.SHANNON_BUNDLE;
const nativePages=new Set(['clock','broadcast','downloads','story','wechat','notes','gesture','components','runtime']);
const entries={
        main: fileURLToPath(new URL("./index.html", import.meta.url)),
        fx: fileURLToPath(new URL("./fx.html", import.meta.url)),
        clock: fileURLToPath(new URL("./clock.html", import.meta.url)),
        broadcast: fileURLToPath(new URL("./broadcast.html", import.meta.url)),
        downloads: fileURLToPath(new URL("./downloads.html", import.meta.url)),
        story: fileURLToPath(new URL("./story.html", import.meta.url)),
        wechat: fileURLToPath(new URL("./wechat.html", import.meta.url)),
        notes: fileURLToPath(new URL("./notes.html", import.meta.url)),
        gesture: fileURLToPath(new URL("./gesture-status.html", import.meta.url)),
        components: fileURLToPath(new URL("./components.html", import.meta.url)),
        pluginManager: fileURLToPath(new URL("./plugin-manager.html", import.meta.url)),
        plugins: fileURLToPath(new URL("./plugins.html", import.meta.url)),
        runtime:fileURLToPath(new URL("./runtime.html",import.meta.url)), };
const input=Object.fromEntries(Object.entries(entries).filter(([key])=>mode==='core'?!nativePages.has(key):mode==='runtime'?nativePages.has(key)||key==='fx':true));
export default defineConfig({resolve:{alias:mode==='core'?[{find:'./voice-input.js',replacement:fileURLToPath(new URL('./src/voice-input-host.js',import.meta.url))}]:[]},clearScreen:false,server:{port:1420,strictPort:true},build:{outDir:mode==='core'?'dist-core':mode==='runtime'?'dist-runtime':'dist',rollupOptions:{input}}});
