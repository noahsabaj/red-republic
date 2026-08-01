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

    /// <summary>Say what the republic refused, or clear it.</summary>
    public event Action<string>? Refused;

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

            // <b>Which thing is worst, before the score.</b> Asked of the
            // simulation rather than worked out again here: this panel used to
            // take the smallest component, which disagrees with the answer the
            // suite tests — an unweighted minimum says a town with no cinema is
            // worse off than one with no food, because it does not know that
            // food is weighted four times as heavily. Two answers to one
            // question, and the tested one had no callers at all.
            var content = _world.Buildings.ContentmentAt(b);
            var worst = content.Worst(t);
            Line("worst want", worst ?? "wants for nothing");
            Line("contentment", $"{content.Overall(t) * 100.0:F0}%");
        }

        Holding(b);
        Orders(b, kind);
    }

    /// <summary>
    /// What the player can do to this building.
    /// </summary>
    /// <remarks>
    /// <b>The panel that answers "why has it stopped" is where the answer is
    /// acted on.</b> Every control here is a verb the simulation already had
    /// and no screen offered: a roster the player could read and not set, a
    /// crew they could see stuck on a site and not call off, a Construction
    /// Office they could build and never hire into, a building they could put
    /// up and never pull down.
    /// </remarks>
    private void Orders(int b, int kind)
    {
        var t = _world.Tables;
        var id = _world.Buildings.IdAt(b);

        _column.AddChild(Parts.Rule());
        _column.AddChild(Parts.Say("ORDERS", "Stamp"));

        if (t.BWorkers[kind] > 0)
        {
            Stepper("crews", $"{_world.Buildings.ShiftsAt(b)}",
                by => Ask(Command.SetShifts(id, _world.Buildings.ShiftsAt(b) + (int)by)));

            Stepper("working day", $"{_world.Buildings.HoursAt(b):F1} h",
                by => Ask(Command.SetBuildingHours(id, _world.Buildings.HoursAt(b) + by)));

            Choice("standing", _world.Buildings.PriorityAt(b).ToString(), () =>
            {
                var next = _world.Buildings.PriorityAt(b) switch
                {
                    Priority.Last => Priority.Ordinary,
                    Priority.Ordinary => Priority.First,
                    _ => Priority.Last,
                };

                Ask(Command.SetPriority(id, next));
            });
        }

        // What a terminal or a distribution office keeps on hand — the standing
        // order that is the only reason anything is ever delivered to one.
        if (t.BStoresToOrder[kind])
        {
            for (var r = 0; r < t.Resources.Length; r++)
            {
                if (!_world.Buildings.Takes(b, r))
                {
                    continue;
                }

                var resource = r;
                var ordered = _world.Buildings.Orders.Get(b, r);
                if (ordered <= 0.0 && _world.Buildings.Stock.Get(b, r) <= 0.0)
                {
                    continue;
                }

                Stepper($"keep {t.ResourceNames[r]}", Parts.Bulk(ordered), by =>
                    Ask(Command.SetStandingOrder(
                        id, resource, Math.Max(0.0, ordered + (by * 10.0)))));
            }

            Choice("order something", "CHOOSE", () => Ordering(b, id));
        }

        // Builders bought from a bloc, for an office. They arrive at a frontier
        // post and a bus has to go and fetch them.
        if (t.BuildingIds[kind] == "ConstructionOffice" && _world.Buildings.IsBuilt(b))
        {
            foreach (var market in new[] { Market.East, Market.West })
            {
                var bloc = market;
                Choice($"hire from the {market}", $"{t.HiringFee * 10:F0} for ten",
                    () => Ask(Command.HireForeign(bloc, id, 10)));
            }
        }

        // Where an unfinished site buys what the republic cannot make.
        if (!_world.Buildings.IsBuilt(b))
        {
            var site = Destination.Building(id);
            var through = _world.BuildPolicy.CrossingFor(site);
            Choice("imports through",
                through is null ? "nothing" : $"post {through}",
                () => Ask(Next(site, through)));

            if (_world.BuildPolicy.IsOverridden(site))
            {
                Choice("follow the default", "CLEAR",
                    () => Ask(Command.ClearImportPolicy(site)));
            }
        }

        // A gang on the site, and the only way to get them off it.
        if (_world.Crews.WorkingAt(Destination.Building(id)) is not null)
        {
            Choice("call the crew off", "RECALL",
                () => Ask(Command.RecallCrew(Destination.Building(id))));
        }

        Choice("pull it down", "DEMOLISH", () => Ask(Command.Demolish(id)));
    }

    /// <summary>
    /// The next post round, so one control cycles every crossing and back to
    /// importing nothing.
    /// </summary>
    private Command Next(Destination site, int? through)
    {
        var posts = _world.Frontier.Crossings;
        if (posts.Count == 0)
        {
            return Command.SetImportPolicy(site, null);
        }

        if (through is null)
        {
            return Command.SetImportPolicy(site, posts[0].Id);
        }

        for (var i = 0; i < posts.Count; i++)
        {
            if (posts[i].Id == through)
            {
                return i + 1 < posts.Count
                    ? Command.SetImportPolicy(site, posts[i + 1].Id)
                    : Command.SetImportPolicy(site, null);
            }
        }

        return Command.SetImportPolicy(site, posts[0].Id);
    }

    /// <summary>Start an order for the first thing this place will take and holds none of.</summary>
    private void Ordering(int b, int id)
    {
        var t = _world.Tables;
        for (var r = 0; r < t.Resources.Length; r++)
        {
            if (_world.Buildings.Takes(b, r) && _world.Buildings.Orders.Get(b, r) <= 0.0)
            {
                Ask(Command.SetStandingOrder(id, r, 10.0));
                return;
            }
        }
    }

    private void Ask(Command command)
    {
        var outcome = _world.Issue(command);
        Refused?.Invoke(outcome.Accepted ? "" : outcome.Refusal);
        Refresh();
    }

    /// <summary>A labelled figure with a minus and a plus beside it.</summary>
    private void Stepper(string label, string value, Action<double> nudge)
    {
        var line = new HBoxContainer { CustomMinimumSize = new Vector2(0.0f, 22.0f) };
        line.AddThemeConstantOverride("separation", Palette.Gap);
        _column.AddChild(line);

        line.AddChild(Parts.Cell(Parts.Say(label, "Faint"), 1.4f));
        line.AddChild(Parts.Cell(Parts.Figure(value), 0.8f, HorizontalAlignment.Right));
        line.AddChild(Parts.Cell(Parts.Stepper(nudge), 0.8f));
    }

    /// <summary>A labelled button.</summary>
    private void Choice(string label, string word, Action press)
    {
        var line = new HBoxContainer { CustomMinimumSize = new Vector2(0.0f, 22.0f) };
        line.AddThemeConstantOverride("separation", Palette.Gap);
        _column.AddChild(line);

        var button = Parts.Press(word, "Quiet");
        button.Pressed += press;

        line.AddChild(Parts.Cell(Parts.Say(label, "Faint"), 1.4f));
        line.AddChild(Parts.Cell(button, 1.2f));
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
