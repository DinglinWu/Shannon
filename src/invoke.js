import {invoke as nativeInvoke} from '@tauri-apps/api/core';
let mode;const runtimeMode=()=>mode??=nativeInvoke('plugin_runtime_mode');
const commands=new Set(['open_notes','close_notes','list_notes','set_note_done','open_clock','clock_active','gesture_control','start_voice_input','stop_voice_input','component_status','component_install']);
export async function invoke(command,args={}){if(commands.has(command)&&!(await runtimeMode()).native)return nativeInvoke('plugin_native_call',{command,args});return nativeInvoke(command,args);}
