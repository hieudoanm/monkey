enum Direction {
  NORTH,
  EAST = 2,
  SOUTH,
  WEST
}

(:glance)
class GlanceView extends WatchUi.GlanceView {
  function onShow() as Void {
    var s = "unicode: héllo — ✓";
    var ch = 'x';
    var hex = 0xFF;
    var f = 1.5e-3;
    var neg = -f;
    var pre = ++counter;
    var post = counter--;
    var bits = (a & b) | (c ^ d) << 2 >>> 1;
    var cmp = a instanceof Lang.Number;
    var nul = null;
    var boo = true || false;
  }
}
