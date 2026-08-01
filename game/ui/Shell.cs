using System;
using System.Collections.Generic;
using Godot;
using RedRepublic.Sim;

namespace RedRepublic.Ui;

/// <summary>What the player is doing with the cursor.</summary>
/// <remarks>
/// A building is one click and a way is two, which is the whole reason this is
/// a state rather than a building kind: a run and a span are lines, and a line
/// has two ends.
/// </remarks>
public enum Cursor
{
    /// <summary>Clicking asks what is there.</summary>
    Idle,

    Building,
    Way,
    Span,
}

/// <summary>
/// The interface: the instrument panel, the screens, and which one is open.
/// </summary>
/// <remarks>
/// <para>
/// <b>One screen at a time, over a paused-looking world.</b> A screen is a sheet
/// of paper on a desk rather than a window floating over the map — the player is
/// an administrator reading returns, and reading is not something you do while
/// glancing at a lorry.
/// </para>
/// <para>
/// <b>Time keeps running while a screen is open</b>, and that is deliberate:
/// real-time is the thesis, and a game that stops whenever you look at a figure
/// is one where thinking is free. The pause key is a separate decision the
/// player makes on purpose.
/// </para>
/// </remarks>
public sealed partial class Shell : CanvasLayer
{
    /// <summary>
    /// The speeds, and what they are called.
    /// </summary>
    /// <remarks>
    /// <b>One real second is one in-game second at speed 1.</b> The faster
    /// settings are a convenience for the hours a republic spends waiting on a
    /// harvest; they are not what the game is balanced at.
    /// </remarks>
    private static readonly (string Name, int Ticks)[] Speeds =
    [
        ("PAUSED", 0),
        ("×1", 1),
        ("×5", 5),
        ("×20", 20),
        ("×60", 60),
    ];

    private readonly Dictionary<Key, Screen> _byKey = [];
    private readonly List<Screen> _screens = [];

    private Hud _hud = null!;
    private Sim.World _world = null!;
    private Screen? _open;
    private MenuScreen _menu = null!;
    private Inspector _inspector = null!;
    private int _speed = 1;
    private double _carried;
    private double _sinceRefresh;

    /// <summary>What the cursor is for at the moment.</summary>
    public Cursor Doing { get; private set; } = Cursor.Idle;

    /// <summary>
    /// What is on the cursor: a building kind, a grade of way, or a kind of
    /// line, depending on <see cref="Doing"/>.
    /// </summary>
    public int Kind { get; private set; } = -1;

    /// <summary>Which bloc's firm is building it, or null for the republic's own crews.</summary>
    public Market? Contracted { get; private set; }

    /// <summary>Whether the way being drawn carries street lighting.</summary>
    public bool Lamps { get; private set; }

    /// <summary>
    /// The first end of a run, once the player has clicked it.
    /// </summary>
    /// <remarks>
    /// A way and a span are the two things ordered by <i>two</i> clicks rather
    /// than one, because they are the two things that are a line rather than a
    /// place.
    /// </remarks>
    public (double X, double Y)? Anchor { get; private set; }

    /// <summary>
    /// Raised when the republic has moved on, so the world can redraw.
    /// </summary>
    /// <remarks>
    /// On the instrument's clock rather than the frame's: nothing the renderer
    /// shows changes faster than a quarter second, and a transform buffer
    /// rewritten every frame costs more than the simulation does.
    /// </remarks>
    public event Action? Ticked;

    /// <summary>
    /// Raised when a save has been taken up, so the world can be rebuilt around
    /// the republic that came back.
    /// </summary>
    public event Action<Sim.World>? TookUp;

    public void Raise(Sim.World world)
    {
        ArgumentNullException.ThrowIfNull(world);
        _world = world;

        _inspector = new Inspector { Name = "Inspector" };
        _inspector.Refused += why => _hud.Refused(why);
        AddChild(_inspector);

        _hud = new Hud { Name = "Hud" };
        AddChild(_hud);
        _hud.Raise(world.Tables);

        var build = new BuildScreen();
        build.Picked += (kind, contractor) =>
        {
            Doing = Cursor.Building;
            Kind = kind;
            Contracted = contractor;
            Anchor = null;
            _hud.Placing(contractor is null
                ? $"Placing a {world.Tables.BName[kind]} — click the ground, Esc to stop"
                : $"Contracting a {world.Tables.BName[kind]} to the {contractor} — "
                    + "click the ground, Esc to stop");
            Shut();
        };

        build.Drawing += (what, kind, lamps) =>
        {
            Doing = what;
            Kind = kind;
            Contracted = null;
            Lamps = lamps;
            Anchor = null;
            _hud.Placing(what == Cursor.Way
                ? $"Laying {world.Tables.Grades[kind].Name} — click where it starts, Esc to stop"
                : $"Stringing a {world.Tables.Utilities[kind].Name} — "
                    + "click where it starts, Esc to stop");
            Shut();
        };

        Add(Key.B, build);
        Add(Key.L, new LabourScreen());
        Add(Key.T, new TradeScreen());
        Add(Key.F, new FinanceScreen());
        Add(Key.J, new JournalScreen());
        Add(Key.R, new ReferenceScreen());

        var menu = new MenuScreen();
        menu.Loaded += taken =>
        {
            _world = taken;
            Shut();
            TookUp?.Invoke(taken);
        };

        Add(Key.M, menu);
        _menu = menu;

        _hud.Keys(Hint());
        _hud.Refresh(world, Speeds[_speed].Name);
    }

    /// <summary>
    /// What the keys do, read off the bindings themselves.
    /// </summary>
    /// <remarks>
    /// So the line the player is shown cannot disagree with what the keys
    /// actually do. Named by the screen's own title, so adding a screen adds
    /// its key to the hint and nothing has to remember to.
    /// </remarks>
    private string Hint()
    {
        var keys = new List<string>
        {
            "WASD pan", "drag to orbit", "wheel to zoom",
        };

        foreach (var (key, screen) in _byKey)
        {
            keys.Add($"{key} {screen.Named.ToLowerInvariant()}");
        }

        keys.Add("Space pause");
        keys.Add($"1-{Speeds.Length - 1} speed");
        keys.Add("Esc back");
        return string.Join(" · ", keys);
    }

    /// <summary>Show one building, or nothing. What clicking the ground does.</summary>
    public void Inspect(int building) => _inspector.Show(_world, building);

    /// <summary>Tell the player what the republic said no to.</summary>
    public void Refused(string why) => _hud.Refused(why);

    /// <summary>Stop placing. What Escape does when something is on the cursor.</summary>
    public void StopPlacing()
    {
        Doing = Cursor.Idle;
        Kind = -1;
        Contracted = null;
        Anchor = null;
        _hud.Placing("");
    }

    /// <summary>
    /// The first end of a run is down; say so and wait for the second.
    /// </summary>
    public void Anchored(double x, double y, string say)
    {
        Anchor = (x, y);
        _hud.Placing(say);
    }

    public override void _Process(double delta)
    {
        // <b>One real second is one in-game second at speed 1</b>, and the
        // remainder is carried rather than dropped: a tick lost every frame is a
        // republic that runs slower than its own clock and nothing would say so.
        _carried += delta * SimClock.TicksPerDay / 86_400.0 * Speeds[_speed].Ticks;
        var due = (int)_carried;
        _carried -= due;

        for (var i = 0; i < due; i++)
        {
            _world.Tick();
        }

        // The instrument refreshes on a clock of its own. Redrawing every figure
        // every frame costs more than the simulation does, and no number here
        // changes fast enough for anybody to see the difference.
        _sinceRefresh += delta;
        if (_sinceRefresh < 0.25)
        {
            return;
        }

        _sinceRefresh = 0.0;
        _hud.Refresh(_world, Speeds[_speed].Name);
        if (_open is { InUse: false })
        {
            _open.Refresh();
        }

        _inspector.Refresh();
        Ticked?.Invoke();
    }

    public override void _UnhandledInput(InputEvent @event)
    {
        if (@event is not InputEventKey key || !key.Pressed || key.Echo)
        {
            return;
        }

        if (key.Keycode == Key.Escape)
        {
            if (_open is not null)
            {
                Shut();
            }
            else if (Doing != Cursor.Idle)
            {
                StopPlacing();
            }
            else
            {
                Show(_menu);
            }

            return;
        }

        if (key.Keycode == Key.Space)
        {
            _speed = _speed == 0 ? 1 : 0;
            _hud.Refresh(_world, Speeds[_speed].Name);
            return;
        }

        if (key.Keycode is >= Key.Key1 and <= Key.Key4)
        {
            _speed = (int)(key.Keycode - Key.Key1) + 1;
            _hud.Refresh(_world, Speeds[_speed].Name);
            return;
        }

        if (_byKey.TryGetValue(key.Keycode, out var screen))
        {
            Show(screen == _open ? null : screen);
        }
    }

    private void Add(Key key, Screen screen)
    {
        // Every screen's refusals go to the same place, so a screen cannot
        // quietly be the one that throws them away.
        screen.Refused += why => _hud.Refused(why);

        AddChild(screen);
        _screens.Add(screen);
        _byKey[key] = screen;
    }

    private void Show(Screen? screen)
    {
        _open?.Shut();
        _open = screen;
        _open?.Open(_world);
    }

    private void Shut() => Show(null);
}
