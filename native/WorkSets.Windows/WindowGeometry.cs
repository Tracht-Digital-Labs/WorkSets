using WorkSets.Core;

namespace WorkSets.Windows;

public static class WindowGeometry
{
    public static RectI32 WorkspaceToScreen(RectI32 rectangle, MonitorRef monitor) => rectangle with
    {
        X = rectangle.X + monitor.WorkArea.X - monitor.Bounds.X,
        Y = rectangle.Y + monitor.WorkArea.Y - monitor.Bounds.Y,
    };

    public static RectI32 ScreenToWorkspace(RectI32 rectangle, MonitorRef monitor) => rectangle with
    {
        X = rectangle.X - (monitor.WorkArea.X - monitor.Bounds.X),
        Y = rectangle.Y - (monitor.WorkArea.Y - monitor.Bounds.Y),
    };

    public static WindowPlacementSpec ToPlacement(RectI32 screenRectangle, MonitorRef monitor, WindowState state)
    {
        var work = monitor.WorkArea;
        return new WindowPlacementSpec
        {
            MonitorId = monitor.Id,
            X = screenRectangle.X,
            Y = screenRectangle.Y,
            Width = Math.Max(1, screenRectangle.Width),
            Height = Math.Max(1, screenRectangle.Height),
            NormalizedX = work.Width == 0 ? 0 : (double)(screenRectangle.X - work.X) / work.Width,
            NormalizedY = work.Height == 0 ? 0 : (double)(screenRectangle.Y - work.Y) / work.Height,
            NormalizedWidth = work.Width == 0 ? 1 : (double)screenRectangle.Width / work.Width,
            NormalizedHeight = work.Height == 0 ? 1 : (double)screenRectangle.Height / work.Height,
            State = state,
        };
    }
}
