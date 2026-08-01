using System;
using Godot;
using RedRepublic.Sim;

namespace RedRepublic.Ui;

/// <summary>
/// The two purses, what is owed on them, and what the blocs are offering.
/// </summary>
/// <remarks>
/// <para>
/// <b>Two currencies, and they are not one converted.</b> A republic starts
/// inside the Eastern Bloc's economy: it can hire Bloc contractors and buy Bloc
/// goods from day one, and everything Western is out of reach until it has
/// exported something for hard currency. The east lends roubles by the hundred
/// thousand over years; the west lends dollars by the thousand over months, and
/// dearer. Which bloc you ask is the decision, and the rung is only how far you
/// are willing to go with it.
/// </para>
/// </remarks>
public sealed partial class FinanceScreen : Screen
{
    private VBoxContainer _ladders = null!;
    private VBoxContainer _repayments = null!;
    private VBoxContainer _tenders = null!;
    private Label _east = null!;
    private Label _west = null!;
    private Label _owedEast = null!;
    private Label _owedWest = null!;

    protected override string Title => "Finance";

    protected override string Blurb =>
        "Roubles and dollars are separate purses. Nothing converts between them "
        + "except selling something across a border.";

    public override void Refresh()
    {
        _east.Text = $"{Parts.Thousands(Republic.Treasury.Of(Market.East))} roubles";
        _west.Text = $"{Parts.Thousands(Republic.Treasury.Of(Market.West))} dollars";
        _owedEast.Text = Owed(Market.East);
        _owedWest.Text = Owed(Market.West);
        Ladders();
        Tenders();
    }

    protected override void Build(VBoxContainer body)
    {
        ArgumentNullException.ThrowIfNull(body);

        var purses = Parts.Columns(body);

        var east = Parts.Card(purses);
        east.AddChild(Parts.Say("EASTERN BLOC", "Stamp"));
        _east = Parts.Say("—", "FigureBig");
        east.AddChild(_east);
        _owedEast = Parts.Say("—", "Faint");
        east.AddChild(_owedEast);

        var west = Parts.Card(purses);
        west.AddChild(Parts.Say("WESTERN BLOC", "Stamp"));
        _west = Parts.Say("—", "FigureBig");
        west.AddChild(_west);
        _owedWest = Parts.Say("—", "Faint");
        west.AddChild(_owedWest);

        var columns = Parts.Columns(body);
        var advances = Parts.Section(columns, "Advances on offer", "One at a time, per bloc.", 1.2f);
        _repayments = new VBoxContainer();
        advances.AddChild(_repayments);
        _ladders = Parts.Scroller(advances);
        _tenders = Parts.Scroller(
            Parts.Section(columns, "Tenders", "Bulk orders the blocs have put on the table.", 1.4f));
    }

    /// <summary>
    /// Paying an advance back, in tenths of what is outstanding.
    /// </summary>
    /// <remarks>
    /// <b>Nothing offered this.</b> A republic could take an advance and had no
    /// way to clear it, so every loan ran to term and defaulted — which burns
    /// the creditor for good, and was the only outcome an advance ever had.
    /// </remarks>
    private void Repayment(VBoxContainer into, Market market)
    {
        var loan = Republic.Loans.Of(market);
        if (loan is null)
        {
            return;
        }

        var line = Parts.Row(into);
        var bloc = market;
        var tenth = Math.Max(1.0, Math.Round(loan.Outstanding / 10.0));

        var pay = Parts.Press($"Repay {Parts.Thousands(tenth)}", "Quiet");
        pay.Pressed += () =>
        {
            Ask(Command.RepayLoan(bloc, tenth));
            Refresh();
        };

        var all = Parts.Press("Clear it", "Primary");
        all.Pressed += () =>
        {
            Ask(Command.RepayLoan(bloc, loan.Outstanding));
            Refresh();
        };

        line.AddChild(Parts.Cell(Parts.Say($"Owed to the {market}"), 1.6f));
        line.AddChild(Parts.Cell(pay, 1.0f));
        line.AddChild(Parts.Cell(all, 1.0f));
    }

    private string Owed(Market market)
    {
        var loan = Republic.Loans.Of(market);
        if (loan is null)
        {
            return Republic.Loans.WillLend(market)
                ? "nothing owed"
                : "defaulted on — this bloc will not advance again";
        }

        var left = loan.DaysLeft(Republic.Clock.DayIndex);
        return $"{Parts.Thousands(loan.Outstanding)} outstanding · {left} days";
    }

    private void Ladders()
    {
        Clear(_repayments);
        foreach (var market in Enum.GetValues<Market>())
        {
            Repayment(_repayments, market);
        }

        Clear(_ladders);
        Parts.Head(
            _ladders,
            new Column("Bloc", 1.0f, HorizontalAlignment.Left),
            new Column("Principal", 1.2f, HorizontalAlignment.Right),
            new Column("Owed back", 1.2f, HorizontalAlignment.Right),
            new Column("Term", 0.8f, HorizontalAlignment.Right),
            new Column("", 1.0f, HorizontalAlignment.Right));

        var alt = false;
        foreach (var market in Enum.GetValues<Market>())
        {
            var ladder = Republic.Loans.Ladder(market);
            for (var tier = 0; tier < ladder.Count; tier++)
            {
                var terms = ladder[tier];
                var line = Parts.Row(_ladders, alt);
                alt = !alt;

                line.AddChild(Parts.Cell(Parts.Say($"{market}"), 1.0f));
                line.AddChild(Parts.Cell(
                    Parts.Figure(Parts.Thousands(terms.Principal)), 1.2f, HorizontalAlignment.Right));
                line.AddChild(Parts.Cell(
                    Parts.Figure(Parts.Thousands(terms.Principal * (1.0 + terms.Interest))),
                    1.2f, HorizontalAlignment.Right));
                line.AddChild(Parts.Cell(
                    Parts.Figure($"{terms.TermDays} d"), 0.8f, HorizontalAlignment.Right));

                var take = Parts.Press("Take", "Quiet");
                var bloc = market;
                var rung = tier;
                // Not disabled into silence. A greyed button says "no" and
                // never says why, and the simulation has six different reasons
                // it might — already owing, defaulted on, no such tier. Let it
                // be pressed, and let it answer.
                take.Pressed += () =>
                {
                    Ask(Command.TakeLoan(bloc, rung));
                    Refresh();
                };

                line.AddChild(Parts.Cell(take, 1.0f));
            }
        }
    }

    private void Tenders()
    {
        Clear(_tenders);
        Parts.Head(
            _tenders,
            new Column("Bloc wants", 1.8f, HorizontalAlignment.Left),
            new Column("Tonnes", 0.9f, HorizontalAlignment.Right),
            new Column("Worth", 1.1f, HorizontalAlignment.Right),
            new Column("By", 0.8f, HorizontalAlignment.Right),
            new Column("", 1.4f, HorizontalAlignment.Right));

        var t = Republic.Tables;
        var any = false;
        foreach (var offer in Republic.Contracts.All)
        {
            any = true;
            var line = Parts.Row(_tenders, offer.Id % 2 == 1);
            line.AddChild(Parts.Cell(
                Parts.Say($"{offer.Market}: {t.ResourceNames[offer.Resource]}"), 1.8f));
            line.AddChild(Parts.Cell(
                Parts.Figure(Parts.Clean(offer.Tonnes)), 0.9f, HorizontalAlignment.Right));
            line.AddChild(Parts.Cell(
                Parts.Figure(Parts.Thousands(offer.Value)), 1.1f, HorizontalAlignment.Right));
            line.AddChild(Parts.Cell(
                Parts.Figure($"{offer.DeadlineDay - Republic.Clock.DayIndex} d"),
                0.8f, HorizontalAlignment.Right));

            if (offer.State == ContractState.Offered)
            {
                var buttons = new HBoxContainer { Alignment = BoxContainer.AlignmentMode.End };
                buttons.AddThemeConstantOverride("separation", Palette.GapTight);

                var id = offer.Id;
                var accept = Parts.Press("Accept", "Primary");
                accept.Pressed += () =>
                {
                    Ask(Command.AcceptContract(id));
                    Refresh();
                };

                var decline = Parts.Press("Decline", "Quiet");
                decline.Pressed += () =>
                {
                    Ask(Command.DeclineContract(id));
                    Refresh();
                };

                buttons.AddChild(accept);
                buttons.AddChild(decline);
                line.AddChild(Parts.Cell(buttons, 1.4f));
            }
            else
            {
                line.AddChild(Parts.Cell(
                    Parts.Say(
                        $"{offer.State} · {Parts.Clean(offer.Delivered)} delivered", "Small"),
                    1.4f, HorizontalAlignment.Right));
            }
        }

        if (!any)
        {
            _tenders.AddChild(Parts.Prose(
                "Nothing is on the table. A tender needs somewhere to land — no crossing, "
                + "no trade.",
                "Faint"));
        }
    }
}
