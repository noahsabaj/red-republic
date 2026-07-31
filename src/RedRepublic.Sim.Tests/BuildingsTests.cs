namespace RedRepublic.Sim.Tests;

/// <summary>
/// Buildings: sites, rosters, storage and the labour plan's running order.
/// </summary>
public sealed class BuildingsTests
{
    private static Tables T => Fixtures.Tables;

    private static Buildings Fresh() => new(T);

    private static int Mine(Buildings b) => b.Add(T.BuildingIndex("CoalMine"), 500.0, 500.0);

    /// <summary>
    /// A site produces nothing, employs nobody and houses nobody until the work
    /// is done. That is what makes construction physical rather than a purchase.
    /// </summary>
    [Fact]
    public void A_site_is_not_a_building_until_the_work_is_done()
    {
        var b = Fresh();
        var i = Mine(b);
        var labour = T.BLabour[b.KindAt(i)];

        Assert.False(b.IsBuilt(i));
        Assert.Equal(0.0, b.Progress(i));
        Assert.Equal(1.0, b.WorkLeft(i));
        Assert.Equal(0, b.Housing());

        b.AddWork(i, labour / 2.0);
        Assert.False(b.IsBuilt(i));
        Assert.Equal(0.5, b.Progress(i), 12);

        b.AddWork(i, labour / 2.0);
        Assert.True(b.IsBuilt(i));
        Assert.Equal(1.0, b.Progress(i));
        Assert.Equal(0.0, b.WorkLeft(i));
    }

    /// <summary>
    /// <b>The bill falls as the work is done.</b> An earlier version demanded the
    /// whole bill at every moment, so a site worked one tick and then stalled
    /// until freight topped it back up — twice the bill over its life, and a
    /// build-out that crawled for a reason nothing in the construction code
    /// showed.
    /// </summary>
    [Fact]
    public void The_bill_of_materials_falls_as_the_work_is_done()
    {
        var b = Fresh();
        var i = Mine(b);
        var kind = b.KindAt(i);
        var bricks = T.ResourceIndex("Bricks");
        var full = 0.0;
        var res = T.Materials.KeysOf(kind);
        var qty = T.Materials.ValuesOf(kind);
        for (var m = 0; m < res.Length; m++)
        {
            if (res[m] == bricks)
            {
                full = qty[m];
            }
        }

        Assert.True(full > 0.0, "a coal mine's bill should include bricks");

        // Nothing delivered: it waits.
        Assert.False(b.HasMaterials(i));
        Assert.Equal(full, b.MaterialOutstanding(i, bricks));

        // The whole bill delivered: it can work.
        foreach (var m in Enumerable.Range(0, res.Length))
        {
            b.Stock.Add(i, res[m], qty[m]);
        }

        Assert.True(b.HasMaterials(i));

        // Half built and half the bill consumed — it must still read as
        // supplied, not as short.
        b.AddWork(i, T.BLabour[kind] / 2.0);
        for (var m = 0; m < res.Length; m++)
        {
            b.Stock.Take(i, res[m], qty[m] / 2.0);
        }

        Assert.True(b.HasMaterials(i));
        Assert.Equal(0.0, b.MaterialOutstanding(i, bricks));
    }

    /// <summary>
    /// A long shift and an extra crew are different decisions. Three crews make
    /// three times the goods and need three times the people; hours past the
    /// standard stretch one crew's day and are capped at a day in total.
    /// </summary>
    [Fact]
    public void Shifts_and_hours_are_different_decisions()
    {
        var b = Fresh();
        var i = Mine(b);
        var id = b.IdAt(i);
        var workers = T.BWorkers[b.KindAt(i)];

        // One standard crew is what every authored rate means.
        Assert.Equal(workers, b.Jobs(i));
        Assert.Equal(T.StandardHours, b.HoursCovered(i));
        Assert.Equal(1.0, b.DayShare(i));

        b.SetStaff(i, workers);
        Assert.Equal(1.0, b.Staffing(i));
        Assert.Equal(1.0, b.Activity(i));

        // Three crews: three times the jobs and three times the work.
        b.SetShiftCount(id, 3);
        Assert.Equal(workers * 3, b.Jobs(i));
        Assert.Equal(3.0, b.DayShare(i));
        b.SetStaff(i, workers * 3);
        Assert.Equal(3.0, b.Activity(i));

        // Half staffed is half the work, not none.
        b.SetStaff(i, workers * 3 / 2);
        Assert.Equal(1.5, b.Activity(i), 1);

        // Three twelve-hour shifts is still one day.
        b.SetNationalHours(12.0);
        Assert.Equal(24.0, b.HoursCovered(i));
        Assert.Equal(3.0, b.DayShare(i));

        // Zero mothballs the place.
        b.SetShiftCount(id, 0);
        Assert.Equal(0, b.Jobs(i));
        Assert.Equal(0.0, b.Staffing(i));
        Assert.Equal(0.0, b.Activity(i));
    }

    /// <summary>
    /// A house has no roster and is never short-staffed; a workplace the player
    /// has closed is a different thing and answers zero. Conflating them would
    /// make every home in the republic read as a stalled factory.
    /// </summary>
    [Fact]
    public void Something_with_no_jobs_is_not_short_staffed()
    {
        var b = Fresh();
        var house = b.Add(T.BuildingIndex("House"), 100.0, 100.0);

        Assert.Equal(0, T.BWorkers[b.KindAt(house)]);
        Assert.Equal(1.0, b.Staffing(house));
    }

    /// <summary>
    /// One ordinary day shift never travels in the dark, which is what keeps
    /// street lighting something a republic grows into rather than a tax on the
    /// opening.
    /// </summary>
    [Fact]
    public void Only_a_long_roster_works_after_dark()
    {
        var b = Fresh();
        var i = Mine(b);
        var id = b.IdAt(i);

        Assert.False(b.WorksAfterDark(i));

        b.SetShiftCount(id, 2);
        Assert.True(b.WorksAfterDark(i));

        b.SetShiftCount(id, 1);
        b.SetBuildingHours(id, 14.0);
        Assert.True(b.WorksAfterDark(i));
    }

    /// <summary>
    /// A rule about a kind covers buildings not yet built — the difference
    /// between a policy and a batch edit — and a rule about one building beats
    /// it. Clearing falls back rather than freezing the number in force.
    /// </summary>
    [Fact]
    public void A_shift_rule_about_a_kind_covers_what_is_built_later()
    {
        var b = Fresh();
        var clinic = T.BuildingIndex("Clinic");

        b.SetKindHours(clinic, 12.0);
        var built = b.Add(clinic, 200.0, 200.0);
        Assert.Equal(12.0, b.HoursAt(built));

        // The building's own rule beats its kind's.
        b.SetBuildingHours(b.IdAt(built), 14.0);
        Assert.Equal(14.0, b.HoursAt(built));

        // Cleared, it falls back to the kind — not to whatever was in force.
        b.SetBuildingHours(b.IdAt(built), null);
        Assert.Equal(12.0, b.HoursAt(built));

        // Cleared again, to the national standard.
        b.SetKindHours(clinic, null);
        Assert.Equal(T.StandardHours, b.HoursAt(built));

        // And hours are clamped to what a person can be asked to work.
        b.SetNationalHours(100.0);
        Assert.Equal(T.MaxHours, b.Shifts.National);
        b.SetNationalHours(1.0);
        Assert.Equal(T.MinHours, b.Shifts.National);
        b.SetNationalHours(double.NaN);
        Assert.Equal(T.StandardHours, b.Shifts.National);
    }

    /// <summary>
    /// Storage is a decision rather than a number: a tank holds liquids and
    /// nothing else. A site under construction is exempt — a bill of materials is
    /// the building arriving in pieces, and a tank that could not accept the
    /// bricks it is made of could never be built.
    /// </summary>
    [Fact]
    public void A_tank_holds_liquids_but_a_tank_site_holds_its_own_bricks()
    {
        var b = Fresh();
        var i = b.Add(T.BuildingIndex("StorageTank"), 300.0, 300.0);
        var oil = T.ResourceIndex("Oil");
        var bricks = T.ResourceIndex("Bricks");

        // Under construction it takes whatever it is made of.
        Assert.True(b.Accepts(i, bricks));
        Assert.True(b.IntakeCapacity(i, bricks) > 0.0);

        b.AddWork(i, T.BLabour[b.KindAt(i)]);
        Assert.True(b.IsBuilt(i));

        // Built, it will hold liquids and refuses the bricks it was made of.
        Assert.True(b.Accepts(i, oil));
        Assert.False(b.Accepts(i, bricks));
        Assert.Equal(0.0, b.IntakeCapacity(i, bricks));

        // A tank keeps goods to order, so it takes nothing until it is told to
        // keep something — the rule that makes a standing order what gives a
        // store a demand of its own.
        Assert.Equal(0.0, b.IntakeCapacity(i, oil));
        b.Orders.Add(i, oil, 120.0);
        Assert.Equal(120.0, b.IntakeCapacity(i, oil));
    }

    /// <summary>
    /// Nothing wants to deliver to a station: it consumes nothing and sells
    /// nothing. A standing order is what gives it a demand of its own.
    /// </summary>
    [Fact]
    public void A_terminal_only_takes_what_it_has_been_told_to_keep()
    {
        var b = Fresh();
        var i = b.Add(T.BuildingIndex("Warehouse"), 400.0, 400.0);
        b.AddWork(i, T.BLabour[b.KindAt(i)]);

        // Bricks, not coal: a warehouse admits covered and open goods, and coal
        // is an aggregate that belongs in a heap. Asking it to hold coal tests
        // the admissions rule rather than the standing order.
        var bricks = T.ResourceIndex("Bricks");
        var coal = T.ResourceIndex("Coal");
        Assert.True(T.BStoresToOrder[b.KindAt(i)]);
        Assert.False(b.Accepts(i, coal));

        Assert.Equal(0.0, b.IntakeCapacity(i, bricks));
        b.Orders.Add(i, bricks, 50.0);
        Assert.Equal(50.0, b.IntakeCapacity(i, bricks));

        // And an order larger than the shed cannot conjure room.
        b.Orders.Set(i, bricks, 10_000.0);
        Assert.Equal(b.StorageCap(i), b.IntakeCapacity(i, bricks));
    }

    /// <summary>
    /// A contracted site is somebody else's problem until it opens: the firm
    /// brings its own materials, and a lorry sent there would be wasted.
    /// </summary>
    [Fact]
    public void Nothing_is_delivered_to_a_contracted_site()
    {
        var b = Fresh();
        var i = Mine(b);
        var bricks = T.ResourceIndex("Bricks");

        Assert.True(b.IntakeCapacity(i, bricks) > 0.0);
        b.SetContractor(i, 0);
        Assert.Equal(0.0, b.IntakeCapacity(i, bricks));

        // Once it opens it is an ordinary building again.
        b.AddWork(i, T.BLabour[b.KindAt(i)]);
        Assert.True(b.IntakeCapacity(i, T.ResourceIndex("Coal")) > 0.0);
    }

    /// <summary>
    /// The running order is the player's central decision, and it has to be
    /// stable: ties break by id so two runs of the same republic fill the same
    /// jobs in the same order.
    /// </summary>
    [Fact]
    public void The_labour_plan_runs_in_priority_then_id_order()
    {
        var b = Fresh();
        var mine = Mine(b);
        var sawmill = b.Add(T.BuildingIndex("Sawmill"), 700.0, 700.0);
        var second = b.Add(T.BuildingIndex("Sawmill"), 900.0, 900.0);
        b.Add(T.BuildingIndex("House"), 100.0, 100.0);

        var order = b.ByStanding();

        // The house has no jobs and is not in the plan at all.
        Assert.Equal(3, order.Count);

        // A mine opens as First — a republic that does not dig does not eat.
        Assert.Equal(Priority.First, b.PriorityAt(mine));
        Assert.Equal(mine, order[0]);

        // Equal priority ties by id, so the order is stable.
        Assert.Equal(b.PriorityAt(sawmill), b.PriorityAt(second));
        Assert.True(order.IndexOf(sawmill) < order.IndexOf(second));

        // And the player can disagree with the table.
        b.SetPriority(b.IdAt(second), Priority.First);
        var moved = b.ByStanding();
        Assert.True(moved.IndexOf(second) < moved.IndexOf(sawmill));
    }

    /// <summary>
    /// Ids are never reused, so a save, a journal entry or a command that names
    /// a building always means the same one — even after something between them
    /// was pulled down.
    /// </summary>
    [Fact]
    public void An_id_is_never_reused_and_the_stock_table_stays_in_step()
    {
        var b = Fresh();
        var coal = T.ResourceIndex("Coal");

        var first = b.Add(T.BuildingIndex("CoalMine"), 100.0, 100.0);
        var second = b.Add(T.BuildingIndex("Sawmill"), 300.0, 300.0);
        var third = b.Add(T.BuildingIndex("Brickworks"), 500.0, 500.0);
        b.Stock.Add(first, coal, 10.0);
        b.Stock.Add(second, coal, 20.0);
        b.Stock.Add(third, coal, 30.0);

        var thirdId = b.IdAt(third);
        Assert.True(b.Demolish(b.IdAt(second)));
        Assert.Equal(2, b.Count);
        Assert.Equal(3, b.Commissioned);

        // The survivor kept its own stock rather than inheriting a neighbour's.
        var moved = b.IndexOf(thirdId);
        Assert.Equal(1, moved);
        Assert.Equal(30.0, b.Stock.Get(moved, coal));
        Assert.Equal(10.0, b.Stock.Get(0, coal));

        // A new building gets a fresh id and empty bins.
        var fourth = b.Add(T.BuildingIndex("Sawmill"), 700.0, 700.0);
        Assert.Equal(4, b.IdAt(fourth));
        Assert.Equal(0.0, b.Stock.Get(fourth, coal));
        Assert.Equal(-1, b.IndexOf(2));
    }

    [Fact]
    public void Footprints_overlap_only_when_they_really_do()
    {
        var b = Fresh();
        var kind = T.BuildingIndex("CoalMine");
        var w = T.BWidth[kind];

        var a = b.Add(kind, 500.0, 500.0);
        var touching = b.Add(kind, 500.0 + w, 500.0);
        var apart = b.Add(kind, 500.0 + (w * 2.0), 500.0);
        var over = b.Add(kind, 500.0 + (w / 4.0), 500.0);

        Assert.False(b.Overlaps(a, touching));
        Assert.False(b.Overlaps(a, apart));
        Assert.True(b.Overlaps(a, over));
        Assert.True(b.Overlaps(over, a));
    }
}
