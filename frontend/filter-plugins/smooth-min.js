'use strict';
// 取樣率轉換濾波器：平滑，最小相位 (Smooth, Minimum Phase)
module.exports = {
  id: 'smooth-min',
  label: '平滑，最小相位',
  sound: '無預響鈴且極低後響鈴，最符合人耳聽覺物理掩蔽效應，類比黑膠般溫潤耐聽',
  kind: 'fir',
  builtin: true
};
