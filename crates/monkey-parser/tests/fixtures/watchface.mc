using Toybox.Graphics;
using Toybox.System as Sys;
using Toybox.WatchUi;

(:background)
class ExampleView extends WatchUi.WatchFace {
  var counter = 0;
  private var label = "hi";
  const MAX = 10;

  function initialize() {
    WatchFace.initialize();
  }

  function onLayout(dc as Dc) as Void {
    setLayout(Rez.Layouts.ExampleLayout(dc));
  }

  function onUpdate(dc as Dc) as Void {
    var screenWidth = dc.getWidth();
    var screenHeight = dc.getHeight();
    var radius = 240 / 3.14;

    dc.setColor(Graphics.COLOR_WHITE, Graphics.COLOR_BLACK);
    dc.clear();
    dc.drawText(
    screenWidth / 2,
    screenHeight / 2,
    Graphics.FONT_SYSTEM_SMALL,
    "Hello Garmin",
    Graphics.TEXT_JUSTIFY_CENTER
    );

    counter += 1;
    if (counter > 10 && Sys.getClockTime().sec == 0) {
      counter = 0;
    } else if (counter == 5) {
      counter = -1;
    } else {
      counter += 2;
    }

    var i = 0;
    while (i < 3) {
      i += 1;
    }
    for (var j = 0; j < 2; j++) {
      Sys.println(j);
    }
    do {
      i -= 1;
    } while (i > 0);

    try {
      mayThrow();
    } catch (e) {
      Sys.println(e);
    } finally {
      cleanup();
    }

    switch (counter) {
    case 0:
      break;
    case 1:
    default:
      counter = 2;
    }

    var arr = [1, 2, 3];
    var d = {
      "a": 1, "b": counter
    };
    var t = counter > 0 ? "pos": "neg";
    var n = new Toybox.Lang.Dictionary();
    var c = (counter as Lang.Number);
    var b = !done && flag || other;
    arr[0] = arr[1] + 1;
    return;
  }

  function helper(x as Lang.Number, y) as Lang.Boolean {
    return x > 0;
  }
}
