using System;
using Godot;
using RedRepublic.Sim;

namespace RedRepublic.Ui;

/// <summary>
/// The standing instructions to the customs houses.
/// </summary>
/// <remarks>
/// <para>
/// <b>The order of the rules is the decision.</b> When throughput or money runs
/// short the first rule is served first, which is why moving one up or down is a
/// verb of its own rather than a re-send of the whole policy — and why this
/// screen is a ranked list rather than a set of switches.
/// </para>
/// <para>
/// <b>Trade is geographic.</b> A rule only reaches goods that are at a customs
/// house within range of a post, and a post settles in its own bloc's money. So
/// trading west means hauling west, and which blocs a posting can reach is a
/// fact about the land rather than a dropdown.
/// </para>
/// </remarks>
public sealed partial class TradeScreen : Screen
{
    private VBoxContainer _rules = null!;
    private VBoxContainer _houses = null!;
    private OptionButton _resource = null!;
    private OptionButton _market = null!;
    private OptionButton _action = null!;

    protected override string Title => "Foreign Trade";

    protected override string Blurb =>
        "Served in order, first rule first. A rule reaches only what stands at a "
        + "customs house, and a house settles in the money of the bloc it faces.";

    public override void Refresh()
    {
        Rules();
        Houses();
    }

    protected override void Build(VBoxContainer body)
    {
        ArgumentNullException.ThrowIfNull(body);

        var adding = Parts.Row(body);
        _resource = new OptionButton();
        foreach (var name in Republic.Tables.ResourceNames)
        {
            _resource.AddItem(name);
        }

        _market = new OptionButton();
        _market.AddItem("Eastern Bloc");
        _market.AddItem("Western Bloc");

        _action = new OptionButton();
        _action.AddItem("Buy");
        _action.AddItem("Sell");

        adding.AddChild(Parts.Cell(_resource, 1.6f));
        adding.AddChild(Parts.Cell(_market, 1.2f));
        adding.AddChild(Parts.Cell(_action, 0.9f));

        var add = Parts.Press("Add the rule", "Primary");
        add.Pressed += Add;
        adding.AddChild(Parts.Cell(add, 1.2f));

        var columns = Parts.Columns(body);
        _rules = Parts.Scroller(
            Parts.Section(columns, "Standing instructions", "In the order they are served.", 1.6f));
        _houses = Parts.Scroller(
            Parts.Section(columns, "Customs houses", "Where goods actually cross.", 1.0f));
    }

    private void Add()
    {
        var outcome = Republic.Issue(Command.AddTradeRule(
            _resource.Selected, (Market)_market.Selected, (TradeAction)_action.Selected));

        if (outcome.Accepted)
        {
            Refresh();
        }
    }

    private void Rules()
    {
        Clear(_rules);
        Parts.Head(
            _rules,
            new Column("#", 0.4f, HorizontalAlignment.Right),
            new Column("Goods", 1.6f, HorizontalAlignment.Left),
            new Column("Bloc", 1.2f, HorizontalAlignment.Left),
            new Column("Doing", 0.8f, HorizontalAlignment.Left),
            new Column("", 1.4f, HorizontalAlignment.Right));

        var t = Republic.Tables;
        for (var i = 0; i < Republic.TradeRules.Count; i++)
        {
            var rule = Republic.TradeRules[i];
            var at = i;
            var line = Parts.Row(_rules, i % 2 == 1);

            line.AddChild(Parts.Cell(Parts.Figure($"{i + 1}"), 0.4f, HorizontalAlignment.Right));
            line.AddChild(Parts.Cell(Parts.Say(t.ResourceNames[rule.Resource]), 1.6f));
            line.AddChild(Parts.Cell(Parts.Say($"{rule.Market}"), 1.2f));
            line.AddChild(Parts.Cell(Parts.Say($"{rule.Action}"), 0.8f));

            var buttons = new HBoxContainer { Alignment = BoxContainer.AlignmentMode.End };
            buttons.AddThemeConstantOverride("separation", 2);

            var up = Parts.Press("▲", "Step");
            up.Disabled = at == 0;
            up.Pressed += () => Move(at, at - 1);
            buttons.AddChild(up);

            var down = Parts.Press("▼", "Step");
            down.Disabled = at >= Republic.TradeRules.Count - 1;
            down.Pressed += () => Move(at, at + 1);
            buttons.AddChild(down);

            var drop = Parts.Press("✕", "Step");
            drop.Pressed += () =>
            {
                Republic.Issue(Command.RemoveTradeRule(at));
                Refresh();
            };

            buttons.AddChild(drop);
            line.AddChild(Parts.Cell(buttons, 1.4f));
        }

        if (Republic.TradeRules.Count == 0)
        {
            _rules.AddChild(Parts.Prose(
                "No instruction stands. Nothing crosses the border in either direction.",
                "Faint"));
        }
    }

    private void Move(int from, int to)
    {
        Republic.Issue(Command.MoveTradeRule(from, to));
        Refresh();
    }

    private void Houses()
    {
        Clear(_houses);
        Parts.Head(
            _houses,
            new Column("House", 1.6f, HorizontalAlignment.Left),
            new Column("Faces", 1.0f, HorizontalAlignment.Left),
            new Column("Staff", 0.8f, HorizontalAlignment.Right));

        var t = Republic.Tables;
        var any = false;
        for (var b = 0; b < Republic.Buildings.Count; b++)
        {
            var kind = Republic.Buildings.KindAt(b);
            if (!Republic.Buildings.IsBuilt(b) || t.BuildingIds[kind] != "Customs")
            {
                continue;
            }

            any = true;
            var line = Parts.Row(_houses, b % 2 == 1);
            var bloc = Republic.Frontier.BlocNear(
                Republic.Buildings.XAt(b), Republic.Buildings.YAt(b));

            line.AddChild(Parts.Cell(Parts.Say(t.BName[kind]), 1.6f));
            line.AddChild(Parts.Cell(Parts.Say($"{bloc}"), 1.0f));
            line.AddChild(Parts.Cell(
                Parts.Figure($"{Republic.Buildings.StaffAt(b)}"), 0.8f, HorizontalAlignment.Right));
        }

        if (!any)
        {
            _houses.AddChild(Parts.Prose(
                "No customs house stands, so no instruction can be served whatever it says.",
                "Alarm"));
        }
    }
}
