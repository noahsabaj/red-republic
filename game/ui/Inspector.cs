using System;
using Godot;
using RedRepublic.Sim;

namespace RedRepublic.Ui;

/// <summary>
/// One building: what it is doing, what it holds, and why it has stopped.
/// </summary>
/// <remarks>
/// <para>
/// <b>Tell the player which thing is worst, not what their score is.</b> The
/// limiters are deliberately not folded together in the simulation, so a stalled
/// building can always say <i>which</i> thing stalled it — "no staff" is
/// actionable and "running at 40%" is not. This panel is the whole reason that
/// separation is worth keeping.
/// </para>
/// <para>
/// It sits over the world rather than replacing it, because what a player does
/// next with this panel open is usually to look at where the building is.
/// </para>
/// </remarks>
public sealed partial class Inspector : PanelContainer
{
    private readonly VBoxContainer _column;
    private Sim.World _world = null!;
    private int _building = -1;

    public Inspector()
    {
        ThemeTypeVariation = "Instrument";
        Visible = false;
        SetAnchorsPreset(LayoutPreset.CenterLeft);
        OffsetLeft = 286.0f;
        OffsetRight = 646.0f;
        OffsetTop = -220.0f;
        OffsetBottom = 220.0f;

        _column = new VBoxContainer();
        _column.AddThemeConstantOverride("separation", 2);
        AddChild(_column);
    }

    /// <summary>Show one building, or nothing.</summary>
    public void Show(Sim.World world, int building)
    {
        _world = world;
        _building = building;
        Visible = building >= 0;
        if (Visible)
        {
            Refresh();
        }
    }

    public void Refresh()
    {
        if (!Visible || _world is null)
        {
            return;
        }

        var b = _world.Buildings.IndexOf(_building);
        if (b < 0)
        {
            // Pulled down while the panel was open. Closing is the honest answer:
            // a panel about a building that is gone is a panel about nothing.
            Visible = false;
            return;
        }

        foreach (var child in _column.GetChildren())
        {
            _column.RemoveChild(child);
            child.QueueFree();
        }

        var t = _world.Tables;
        var kind = _world.Buildings.KindAt(b);

        _column.AddChild(Parts.Say(t.BName[kind].ToUpperInvariant(), "Section"));

        if (!_world.Buildings.IsBuilt(b))
        {
            Line("built", $"{_world.Buildings.Progress(b) * 100.0:F0}%");
            var crew = _world.Crews.WorkingAt(Destination.Building(_world.Buildings.IdAt(b)));
            Line("worked by",
                _world.Buildings.ContractorAt(b) >= 0 ? "a Bloc firm"
                : crew is null ? "nobody" : $"{crew.Heads} builders");
            Wants(b, kind);
            return;
        }

        // <b>Which thing is worst.</b> One answer, in the words the simulation
        // uses, rather than a percentage the player has to diagnose.
        var stall = Systems.StallReason(_world, b);
        var doing = Parts.Say(stall switch
        {
            Stall.NoStaff => "nobody works here",
            Stall.NoPower => "no current reaches it",
            Stall.NoInputs => "it has run out of what it needs",
            _ => "working",
        }, stall == Stall.None ? "Good" : "Alarm");

        _column.AddChild(doing);
        _column.AddChild(Parts.Rule());

        if (t.BWorkers[kind] > 0)
        {
            Line("staff", $"{_world.Buildings.StaffAt(b)} / {_world.Buildings.Jobs(b)}");
            Line("crews", $"{_world.Buildings.ShiftsAt(b)}");
            Line("standing", $"{_world.Buildings.PriorityAt(b)}");
            Line("working day", $"{_world.Buildings.HoursAt(b):F1} h");
        }

        if (t.BPowerDraw[kind] > 0.0)
        {
            Line("current", _world.Buildings.PoweredAt(b) ? "reaching it" : "none");
        }

        if (t.BHeat[kind] > 0.0)
        {
            Line("heat", _world.Buildings.HeatedAt(b) ? "reaching it" : "none");
        }

        if (t.BResidents[kind] > 0)
        {
            Line("residents",
                $"{_world.Citizens.ResidentsOf(_world.Buildings.IdAt(b))} / {t.BResidents[kind]}");

            // The wants, worst first — the actionable half of contentment.
            var content = _world.Buildings.ContentmentAt(b);
            var parts = content.Parts;
            var worst = 0;
            for (var i = 1; i < parts.Length; i++)
            {
                if (parts[i] < parts[worst])
                {
                    worst = i;
                }
            }

            Line("contentment", $"{content.Overall(t) * 100.0:F0}%");
            Line("worst want", $"{Contentment.Names[worst]} at {parts[worst] * 100.0:F0}%");
        }

        Holding(b);
    }

    private void Wants(int b, int kind)
    {
        var t = _world.Tables;
        var res = t.Materials.KeysOf(kind);
        if (res.Length == 0)
        {
            return;
        }

        _column.AddChild(Parts.Rule());
        _column.AddChild(Parts.Say("STILL WANTS", "Stamp"));
        foreach (var r in res)
        {
            var outstanding = _world.Buildings.MaterialOutstanding(b, r);
            if (outstanding > 0.0)
            {
                Line(t.ResourceNames[r], Parts.Bulk(outstanding));
            }
        }
    }

    private void Holding(int b)
    {
        var t = _world.Tables;
        var any = false;
        for (var r = 0; r < t.Resources.Length; r++)
        {
            var held = _world.Buildings.Stock.Get(b, r);
            if (held <= 0.0)
            {
                continue;
            }

            if (!any)
            {
                _column.AddChild(Parts.Rule());
                _column.AddChild(Parts.Say("HOLDING", "Stamp"));
                any = true;
            }

            Line(t.ResourceNames[r], Parts.Bulk(held));
        }
    }

    private void Line(string label, string value)
    {
        var line = new HBoxContainer { CustomMinimumSize = new Vector2(0.0f, 19.0f) };
        line.AddThemeConstantOverride("separation", Palette.Gap);
        _column.AddChild(line);

        line.AddChild(Parts.Cell(Parts.Say(label, "Faint"), 1.4f));
        line.AddChild(Parts.Cell(
            Parts.Figure(value), 1.0f, HorizontalAlignment.Right));
    }
}
