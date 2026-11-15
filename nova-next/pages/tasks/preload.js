const { contextBridge, ipcRenderer } = require('electron');

const on = (channel) => (cb) => ipcRenderer.on(channel, (_e, data) => cb(data));

// The Tasks window (1.8.3): the app keeps the list (tasks.json) and checks everything sent here.
contextBridge.exposeInMainWorld('tasks', {
  // {tasks, tags, reminders, lang}
  data: () => ipcRenderer.invoke('tasks-data'),
  onTasks: on('tasks'),
  // {title, notes, due (ms or null), remind (minutes before, or null), tag}; the stored task back
  add: (task) => ipcRenderer.invoke('tasks-add', task),
  update: (id, patch) => ipcRenderer.invoke('tasks-update', id, patch),
  remove: (id) => ipcRenderer.invoke('tasks-remove', id),
  clearDone: () => ipcRenderer.invoke('tasks-clear-done'),
  window: (action) => ipcRenderer.invoke('tasks-window', action),
});
