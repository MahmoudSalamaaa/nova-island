// Lets the app's main-process modules (features.js, settings.js, prayer.js, ...) run under plain Node
// for the parity references: require('electron') returns this stand-in. userData is a temp folder.
const Module = require('module');
const os = require('os');
const path = require('path');
const fs = require('fs');

const userData = fs.mkdtempSync(path.join(os.tmpdir(), 'nova-oracle-'));
const listeners = {};

const electron = {
  app: {
    getPath: (name) => (name === 'userData' ? userData : path.join(userData, name)),
    getVersion: () => '1.7.0',
    isPackaged: true,
    getLocale: () => 'en-US',
    on: (ev, fn) => { (listeners[ev] ||= []).push(fn); },
  },
  ipcMain: { on() {}, handle() {} },
  shell: { openExternal() {}, openPath() {} },
  net: {},
  nativeImage: { createFromPath: () => ({ isEmpty: () => true }) },
  clipboard: { readText: () => '', writeText() {} },
  screen: {},
};

const resolve = Module._resolveFilename;
Module._resolveFilename = function (request, ...rest) {
  if (request === 'electron') return 'electron';
  return resolve.call(this, request, ...rest);
};
require.cache.electron = { id: 'electron', filename: 'electron', loaded: true, exports: electron };

module.exports = { userData, electron };
