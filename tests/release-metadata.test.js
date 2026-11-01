import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
const read=path=>readFileSync(new URL('../'+path,import.meta.url),'utf8').replace(/\r\n/g,'\n');
test('application and package lock versions match the installer release',()=>{
 const pkg=JSON.parse(read('package.json')),lock=JSON.parse(read('package-lock.json')),tauri=JSON.parse(read('src-tauri/tauri.conf.json'));
 const cargo=read('src-tauri/Cargo.toml').match(/^version = "([^"]+)"/m)[1];
 assert.equal(pkg.version,tauri.version);assert.equal(lock.version,pkg.version);assert.equal(lock.packages[''].version,pkg.version);assert.equal(cargo,pkg.version);
 assert.ok(read('src-tauri/Cargo.lock').includes(`name = "shannon"\nversion = "${pkg.version}"`));
});
test('rename retains existing data identity and update service and the existing installer identity',()=>{
 const config=JSON.parse(read('src-tauri/tauri.conf.json'));
 assert.equal(config.productName,'Shannon');assert.equal(config.app.windows[0].title,'闪念');
 assert.equal(config.identifier,'com.brynn.ailauncher');assert.equal(config.plugins.updater.endpoints[0],'https://gitee.com/wu-dinglin/shannon/raw/master/latest.json');
 assert.equal(config.bundle.windows.nsis.installerHooks,undefined);
});
