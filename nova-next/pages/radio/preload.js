const { contextBridge, ipcRenderer } = require('electron');

const on = (channel) => (cb) => ipcRenderer.on(channel, (_e, data) => cb(data));

contextBridge.exposeInMainWorld('radio', {
  // {stations, suwar, now: {station, surah, playing, live}, favorites}
  data: () => ipcRenderer.invoke('radio-data'),
  onState: on('radio-state'),
  // a station live (surah null) or one of a reciter's surahs; the island plays it
  play: (station, surah) => ipcRenderer.send('radio-play', { station, surah: surah || null }),
  favorite: (id, on) => ipcRenderer.send('radio-fav', id, !!on),
  window: (action) => ipcRenderer.send('radio-window', action),
});
