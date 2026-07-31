namespace RedRepublic.Sim;

/// <summary>
/// Metric, and honest about it. Metres and seconds.
/// </summary>
/// <remarks>
/// <para>
/// The Rust build gave each quantity its own newtype so that adding a distance
/// to a duration would not compile. That is worth having and is not what this
/// is: what lives here are the <b>conversions</b> — the places a number changes
/// meaning — gathered in one file rather than scattered as bare arithmetic. A
/// <c>* 3.6</c> written inline somewhere is the smell this exists to prevent.
/// </para>
/// <para>
/// <b>Positions are two doubles, never a Godot <c>Vector2</c>.</b> Godot's
/// vector types hold 32-bit floats. The determinism rule is bit-exactness on
/// doubles, and map generation must reproduce across machines because a shared
/// seed is a promise between players — so a position that passes through a
/// <c>Vector2</c> has been rounded before anything is computed with it, and
/// nothing downstream would ever say so. This project cannot even name that
/// type: it has no reference to Godot, which is the point of the split.
/// </para>
/// <para>
/// The word "tile" does not belong in this vocabulary. The grid describes
/// terrain; it is not the unit things are made of.
/// </para>
/// </remarks>
public static class Units
{
    // ---- speed ----

    /// <summary>
    /// Kilometres per hour is the unit a human quotes a vehicle in; metres per
    /// second is what the simulation computes in.
    /// </summary>
    public static double KphToMps(double kph) => kph / 3.6;

    public static double MpsToKph(double mps) => mps * 3.6;

    /// <summary>
    /// How long a speed takes to cover a distance, in seconds.
    /// </summary>
    /// <remarks>
    /// A zero speed is a caller error rather than an infinity. A stationary
    /// thing never arrives, and returning infinity here would propagate into
    /// arrival times and schedules as a silently poisoned number that surfaces
    /// somewhere else entirely.
    /// </remarks>
    public static double TimeToCover(double mps, double metres)
    {
        ArgumentOutOfRangeException.ThrowIfNegativeOrZero(mps);
        return metres / mps;
    }

    // ---- durations, in seconds ----

    public static double Minutes(double m) => m * 60.0;

    public static double Hours(double h) => h * 3600.0;

    public static double Days(double d) => d * 86400.0;

    public static double AsMinutes(double s) => s / 60.0;

    public static double AsHours(double s) => s / 3600.0;

    public static double AsDays(double s) => s / 86400.0;

    // ---- geometry ----

    /// <summary>
    /// Straight-line distance between two points. Not a travel distance —
    /// routing is a graph problem and this is the geometry underneath it.
    /// </summary>
    /// <remarks>
    /// Deliberately <c>Sqrt</c> and not a hypot. A hypot is a libm compound
    /// function and is permitted to differ in its last bit between platforms and
    /// library versions; <c>Sqrt</c> is exactly rounded by IEEE-754 everywhere.
    /// At map scales <c>dx * dx</c> is nowhere near overflow, so the only thing
    /// a hypot would buy is the non-determinism.
    /// </remarks>
    public static double Distance(double ax, double ay, double bx, double by)
    {
        var dx = ax - bx;
        var dy = ay - by;
        return Math.Sqrt((dx * dx) + (dy * dy));
    }

    /// <summary>Squared distance, for when only an ordering is wanted. Exact, and cheaper.</summary>
    public static double DistanceSquared(double ax, double ay, double bx, double by)
    {
        var dx = ax - bx;
        var dy = ay - by;
        return (dx * dx) + (dy * dy);
    }

    /// <summary>
    /// The perpendicular distance from a point to a segment, or to the nearer end
    /// when the foot of the perpendicular falls outside it.
    /// </summary>
    /// <remarks>
    /// What decides whether a building is beside a line: a factory next to the
    /// middle of a span is next to the span, and one past the end of it is as far
    /// away as the end is rather than as far as the infinite line would be.
    /// </remarks>
    public static double DistanceToSegment(
        double x, double y, double fromX, double fromY, double toX, double toY)
    {
        var dx = toX - fromX;
        var dy = toY - fromY;
        var length2 = (dx * dx) + (dy * dy);
        if (length2 <= double.Epsilon)
        {
            return Distance(x, y, fromX, fromY);
        }

        var along = Math.Clamp(
            (((x - fromX) * dx) + ((y - fromY) * dy)) / length2, 0.0, 1.0);
        return Distance(x, y, fromX + (dx * along), fromY + (dy * along));
    }
}
