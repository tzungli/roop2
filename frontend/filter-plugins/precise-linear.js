'use strict';
// 取樣率轉換濾波器：精準，線性相位 (Precise, Linear Phase)
module.exports = {
  id: 'precise-linear',
  label: '精準，線性相位',
  sound: '極陡峭滾降、-140dB 深度阻帶抑制，絕對零相位失真，最高保真度 (旗艦預設推薦)',
  kind: 'fir',
  builtin: true
};
