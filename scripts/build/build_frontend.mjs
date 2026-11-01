import {spawnSync} from 'node:child_process';
const mode=process.argv[2];if(!['core','runtime'].includes(mode))throw new Error('expected core or runtime');
const result=spawnSync(process.execPath,['node_modules/vite/bin/vite.js','build'],{stdio:'inherit',env:{...process.env,SHANNON_BUNDLE:mode}});process.exit(result.status??1);
