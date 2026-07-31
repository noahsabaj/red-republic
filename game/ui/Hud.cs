using System;
using System.Collections.Generic;
using Godot;
using RedRepublic.Sim;

namespace RedRepublic.Ui;

/// <summary>
/// The republic's instrument panel.
/// </summary>
/// <remarks>
/// <para>
/// <b>Every number here is read from the republic. Nothing in this file works
/// anything out</b> — the moment a panel starts deriving its own answer there are
/// two versions of the balance and only one of them is tested.
/// </para>
/// <para>
/// A strip across the top carries the four things true whatever the player is
/// doing: where they are, what day it is, how fast it is running, and what the
/// weather is about to do. A panel down the left is the state of the republic,
/// as ruled label-and-figure rows under stamped section heads — the same table
/// the screens use, because an instrument and a form should not read as two
/// different applications. Two returns down the right are the stockpiles and the
/// people, the two lists long enough to want scrolling.
/// </para>
/// <para>
/// <b>Long lists are pooled, and that is a measured rule.</b> Building five
/// thousand labels costs a visible hitch while updating them costs nothing — a
/// factor of a hundred and sixty-five. So the tables build their rows once and
/// reuse them, and nothing here creates a node per datum per frame.
/// </para>
/// <para>
/// <b>No literal is ever sized against a roster.</b> The pools are sized from the
/// rosters themselves, because a literal row count beside a roster is a second
/// copy of it — and it is the copy that fails silently: the table stops one row
/// short, nothing errors, and no test covers it because none of it is simulation
/// state.
/// </para>
/// </remarks>
public sealed partial class Hud : CanvasLayer
{
    /// <summary>
    /// How tall a row is here. Tighter than a full screen's, because this panel
    /// sits over the republic and every pixel of it is a pixel of map nobody can
    /// see.
    /// </summary>
    private const int HudRow = 19;

    /// <summary>
    /// The order the census hands them back in. Named here rather than read
    /// across the boundary because these are labels, not simulation facts.
    /// </summary>
    private static readonly string[] StageNames =
        ["Infants", "Pupils", "Students", "Workers", "Retired"];

    private static readonly string[] LearningNames = ["Unschooled", "Schooled", "Graduates"];

    /// <summary>
    /// The rows of the left-hand panel, in order. A section of nothing continues
    /// the one above.
    /// </summary>
    /// <remarks>
    /// A table rather than a run of calls, so the panel's contents are one list
    /// to read and the refresh can write to it by name.
    /// </remarks>
    private static readonly (string Section, string Key, string Label)[] StateRows =
    [
        ("The republic", "people", "people"),
        ("", "standing", "standing"),
        ("", "treasury", "treasury, roubles"),
        ("", "content", "contentment"),
        ("Construction", "sites", "sites"),
        ("", "builders", "builders"),
        ("", "hired", "hired abroad"),
        ("Movement", "lorries", "lorries"),
        ("", "waiting", "at the frontier"),
        ("Networks", "ways", "ways"),
        ("", "lit", "street lighting"),
        ("", "grid", "power & heat"),
        ("", "dark", "not served"),
        ("Weather", "degrees", "temperature"),
        ("", "snow", "snow"),
    ];

    private readonly Dictionary<string, Label> _state = [];
    private readonly List<Label> _stock = [];
    private readonly List<Label> _people = [];

    private Label _clock = null!;
    private Label _speed = null!;
    private Label _weather = null!;
    private Label _hint = null!;
    private Tables _t = null!;

    /// <summary>
    /// The three things that compete for the one line along the bottom.
    /// </summary>
    /// <remarks>
    /// Held apart rather than written over each other, because a placement
    /// refusal must not cost the player the key list and the overlay must not
    /// cost them the refusal.
    /// </remarks>
    private string _hintKeys = "";

    private string _hintPlacing = "";
    private string _hintRefusal = "";

    public void Raise(Tables tables)
    {
        _t = tables;
        Strip();
        StatePanel();
        Returns();
        Hint();
    }

    /// <summary>What the player is being told about what they are doing.</summary>
    public void Placing(string what)
    {
        _hintPlacing = what;
        WriteHint();
    }

    /// <summary>Why the republic said no. Shown until the next thing happens.</summary>
    public void Refused(string why)
    {
        _hintRefusal = why;
        WriteHint();
    }

    public void Refresh(Sim.World world, string speed)
    {
        ArgumentNullException.ThrowIfNull(world);

        var date = world.Clock.Date;
        _clock.Text = $"{date.Year:0000}-{date.Month:00}-{date.Day:00}";
        _speed.Text = speed;

        var degrees = world.Climate.MeanOn(world.Clock.DayOfYear);
        _weather.Text = $"{degrees:F1} °C · {world.Ground.Snow * 100.0:F0}% snow";

        var built = 0;
        var sites = 0;
        var dark = 0;
        for (var b = 0; b < world.Buildings.Count; b++)
        {
            if (world.Buildings.IsBuilt(b))
            {
                built++;
                if (_t.BPowerDraw[world.Buildings.KindAt(b)] > 0.0
                    && !world.Buildings.PoweredAt(b))
                {
                    dark++;
                }
            }
            else
            {
                sites++;
            }
        }

        var builders = 0;
        foreach (var party in world.Crews.All)
        {
            builders += party.Heads;
        }

        var (withLamps, burning) = world.Roads.LitLength();
        var grid = world.Grid.LengthOf(_t.UtilityIndex("Power"))
            + world.Grid.LengthOf(_t.UtilityIndex("Heat"));

        Write("people", Parts.Thousands(world.Citizens.Count));
        Write("standing", $"{built} · {sites} going");
        Write("treasury", Parts.Thousands(world.Treasury.Of(Market.East)));
        Write("content", $"{Contentment(world) * 100.0:F0}%");
        Write("sites", $"{sites + world.RoadWorks.Sites.Count + world.LineWorks.Sites.Count}");
        Write("builders", $"{builders}");
        Write("hired", $"{world.Crews.HiredTotal()}");
        Write("lorries", $"{world.Fleet.Count}");
        Write("waiting", $"{world.Migration.HeadsWaiting()}");
        Write("ways", $"{world.Roads.TotalLength() / 1000.0:F1} km");
        Write("lit", withLamps <= 0.0
            ? "none"
            : $"{burning / 1000.0:F1} / {withLamps / 1000.0:F1} km");
        Write("grid", $"{grid / 1000.0:F1} km");
        Write("dark", dark == 0 ? "—" : $"{dark}");
        Write("degrees", $"{degrees:F1} °C");
        Write("snow", $"{world.Ground.Snow * 100.0:F0}%");

        Stocks(world);
        People(world);
    }

    /// <summary>
    /// How content the republic is, weighted by how many live in each estate.
    /// </summary>
    /// <remarks>
    /// One wretched outpost does not cancel a working city, and an unweighted
    /// mean would say it did. The same weighting the simulation's own migration
    /// pass uses — because two answers to "how is the republic doing" is exactly
    /// the failure this file exists to avoid.
    /// </remarks>
    private double Contentment(Sim.World world)
    {
        var scored = 0.0;
        var heads = 0;
        for (var b = 0; b < world.Buildings.Count; b++)
        {
            if (!world.Buildings.IsBuilt(b) || _t.BResidents[world.Buildings.KindAt(b)] == 0)
            {
                continue;
            }

            var here = world.Citizens.ResidentsOf(world.Buildings.IdAt(b));
            scored += world.Buildings.ContentmentAt(b).Overall(_t) * here;
            heads += here;
        }

        return heads == 0 ? 0.0 : scored / heads;
    }

    private void Write(string key, string value)
    {
        if (_state.TryGetValue(key, out var label))
        {
            label.Text = value;
        }
    }

    private void Strip()
    {
        var panel = new PanelContainer
        {
            ThemeTypeVariation = "Instrument",
            MouseFilter = Control.MouseFilterEnum.Ignore,
        };

        panel.SetAnchorsPreset(Control.LayoutPreset.TopWide);
        panel.OffsetLeft = 12.0f;
        panel.OffsetRight = -12.0f;
        panel.OffsetTop = 10.0f;
        AddChild(panel);

        var line = new HBoxContainer();
        line.AddThemeConstantOverride("separation", Palette.GapSection);
        panel.AddChild(line);

        _clock = Parts.Say("", "Figure");
        _speed = Parts.Say("", "Stamp");
        _weather = Parts.Say("", "Small");

        line.AddChild(Parts.Say("RED REPUBLIC", "Stamp"));
        line.AddChild(_clock);
        line.AddChild(_speed);
        line.AddChild(Parts.Fill());
        line.AddChild(_weather);
    }

    private void StatePanel()
    {
        var panel = new PanelContainer { ThemeTypeVariation = "Instrument" };
        panel.SetAnchorsPreset(Control.LayoutPreset.TopLeft);
        panel.OffsetLeft = 12.0f;
        panel.OffsetRight = 262.0f;
        panel.OffsetTop = 56.0f;
        panel.OffsetBottom = 500.0f;
        AddChild(panel);

        var column = new VBoxContainer();
        column.AddThemeConstantOverride("separation", 1);
        panel.AddChild(column);

        foreach (var (section, key, label) in StateRows)
        {
            if (section.Length > 0)
            {
                if (_state.Count > 0)
                {
                    column.AddChild(Parts.Gap(Palette.GapTight));
                }

                column.AddChild(Parts.Say(section.ToUpperInvariant(), "Stamp"));
            }

            var line = new HBoxContainer
            {
                CustomMinimumSize = new Vector2(0.0f, HudRow),
            };

            line.AddThemeConstantOverride("separation", Palette.Gap);
            column.AddChild(line);

            line.AddChild(Parts.Cell(Parts.Say(label, "Faint"), 1.2f));
            var value = Parts.Figure("—", "Figure");
            line.AddChild(Parts.Cell(value, 1.0f, HorizontalAlignment.Right));
            _state[key] = value;
        }
    }

    /// <summary>
    /// The two returns down the right: what is in the stores, and who there is.
    /// </summary>
    /// <remarks>
    /// Both pools are sized from the rosters themselves. A literal here would be
    /// a second copy of the resource table, and the copy that stops one row short
    /// the day a resource is added.
    /// </remarks>
    private void Returns()
    {
        var panel = new PanelContainer { ThemeTypeVariation = "Instrument" };
        // <b>An anchor preset does not size the control.</b> Anchoring to the
        // right and setting only the right offset leaves the left one at zero, so
        // the panel is zero pixels wide and draws nothing — with no error, and no
        // way to tell it from a panel that was never added. Both edges are given.
        panel.SetAnchorsPreset(Control.LayoutPreset.TopRight);
        panel.OffsetLeft = -242.0f;
        panel.OffsetRight = -12.0f;
        panel.OffsetTop = 56.0f;
        panel.OffsetBottom = 700.0f;
        AddChild(panel);

        var column = new VBoxContainer();
        column.AddThemeConstantOverride("separation", 1);
        panel.AddChild(column);

        column.AddChild(Parts.Say("STORES", "Stamp"));
        for (var r = 0; r < _t.Resources.Length; r++)
        {
            _stock.Add(Return(column, _t.ResourceNames[r]));
        }

        column.AddChild(Parts.Gap(Palette.GapTight));
        column.AddChild(Parts.Say("PEOPLE", "Stamp"));
        foreach (var name in StageNames)
        {
            _people.Add(Return(column, name));
        }

        foreach (var name in LearningNames)
        {
            _people.Add(Return(column, name));
        }
    }

    private static Label Return(Control into, string name)
    {
        var line = new HBoxContainer { CustomMinimumSize = new Vector2(0.0f, HudRow) };
        line.AddThemeConstantOverride("separation", Palette.Gap);
        into.AddChild(line);

        line.AddChild(Parts.Cell(Parts.Say(name, "Faint"), 1.3f));
        var value = Parts.Figure("—", "Figure");
        line.AddChild(Parts.Cell(value, 1.0f, HorizontalAlignment.Right));
        return value;
    }

    private void Stocks(Sim.World world)
    {
        for (var r = 0; r < _stock.Count; r++)
        {
            var held = 0.0;
            for (var b = 0; b < world.Buildings.Count; b++)
            {
                held += world.Buildings.Stock.Get(b, r);
            }

            _stock[r].Text = held <= 0.0 ? "—" : Parts.Bulk(held);

            // <b>Tinted by what the simulation says, which is the one thing a
            // screen may colour for itself</b> — and it reads the palette rather
            // than typing a triple.
            _stock[r].AddThemeColorOverride(
                "font_color", held <= 0.0 ? Palette.PaperFaint : Palette.Paper);
        }
    }

    private void People(Sim.World world)
    {
        var at = 0;
        for (var stage = 0; stage < StageNames.Length && at < _people.Count; stage++, at++)
        {
            _people[at].Text = $"{world.Citizens.ByStage((LifeStage)stage)}";
        }

        for (var taught = 0; taught < LearningNames.Length && at < _people.Count; taught++, at++)
        {
            _people[at].Text = $"{world.Citizens.ByEducation((Education)taught)}";
        }
    }

    private void Hint()
    {
        var panel = new PanelContainer
        {
            ThemeTypeVariation = "Instrument",
            MouseFilter = Control.MouseFilterEnum.Ignore,
        };

        panel.SetAnchorsPreset(Control.LayoutPreset.BottomWide);
        panel.OffsetLeft = 12.0f;
        panel.OffsetRight = -12.0f;
        panel.OffsetTop = -44.0f;
        panel.OffsetBottom = -10.0f;
        AddChild(panel);

        _hint = Parts.Say("", "Faint");
        panel.AddChild(_hint);

        _hintKeys = "WASD pan · drag to orbit · wheel to zoom · "
            + "B build · L labour · T trade · F finance · J journal · Esc menu";
        WriteHint();
    }

    private void WriteHint()
    {
        // A refusal outranks what the player is doing, which outranks the keys:
        // the most specific thing the republic has to say wins the one line.
        _hint.Text = _hintRefusal.Length > 0
            ? _hintRefusal
            : _hintPlacing.Length > 0 ? _hintPlacing : _hintKeys;

        _hint.AddThemeColorOverride(
            "font_color", _hintRefusal.Length > 0 ? Palette.Alarm : Palette.PaperFaint);
    }
}
