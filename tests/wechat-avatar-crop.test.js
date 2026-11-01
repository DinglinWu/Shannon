import test from 'node:test';
import assert from 'node:assert/strict';
import {createCrop,cropRect,panCrop,zoomCrop} from '../src/wechat-avatar-crop.js';
test('starts with a centered square for portrait and landscape photos',()=>{
 assert.deepEqual(cropRect(createCrop(1200,800)),{x:200,y:0,side:800});
 assert.deepEqual(cropRect(createCrop(800,1200)),{x:0,y:200,side:800});
 for(const [w,h] of [[0,3],[-1,2],[NaN,2],[Infinity,1]])assert.throws(()=>createCrop(w,h));
});
test('drag follows displayed image movement and never exposes empty space',()=>{
 let crop=createCrop(1200,800);crop=panCrop(crop,32,0,256);assert.equal(cropRect(crop).x,100);
 crop=panCrop(crop,10000,-10000,256);assert.deepEqual(cropRect(crop),{x:0,y:0,side:800});
 crop=panCrop(crop,-10000,10000,256);assert.equal(cropRect(crop).x,400);
 assert.deepEqual(panCrop(crop,5,5,0),crop);
});
test('zoom preserves focus, clamps at edges, and zooming out covers the whole crop area',()=>{
 let crop=zoomCrop(createCrop(1000,800),2);assert.deepEqual(cropRect(crop),{x:300,y:200,side:400});
 crop=panCrop(crop,10000,10000,256);assert.deepEqual(cropRect(crop),{x:0,y:0,side:400});
 crop=zoomCrop(crop,1);assert.deepEqual(cropRect(crop),{x:0,y:0,side:800});
 assert.equal(zoomCrop(crop,100).zoom,4);assert.equal(zoomCrop(crop,.1).zoom,1);
 for(const zoom of [1,1.5,2,3,4]){const r=cropRect(panCrop(zoomCrop(crop,zoom),-100000,100000,200));assert.ok(r.x>=0&&r.y>=0&&r.x+r.side<=1000&&r.y+r.side<=800);}
});
