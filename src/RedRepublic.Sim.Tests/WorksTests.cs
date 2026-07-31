namespace RedRepublic.Sim.Tests;

/// <summary>Ways and lines: ordered, supplied, and finished.</summary>
public sealed class WorksTests
{
    private static Tables T => Fixtures.Tables;

    private static int GradeIndex(string id) =>
        Array.FindIndex(T.Grades, g => g.Id == id);

    private static int UtilityIndex(string id) =>
        Array.FindIndex(T.Utilities, u => u.Id == id);

    /// <summary>
    /// <b>A road is a site with a bill, and the gravel has to be driven to it.</b>
    /// An order does not conjure a road; it puts down somewhere for lorries to
    /// take gravel.
    /// </summary>
    [Fact]
    public void A_way_is_ordered_as_a_site_with_a_bill()
    {
        var works = new RoadWorks(T);
        var terrain = Terrain.Flat(6000.0, 10.0);
        var gravelRoad = GradeIndex("Gravel");

        var refusal = works.Order(
            1000.0, 1000.0, 3000.0, 1000.0, gravelRoad, false, 0, terrain, out var site);

        Assert.Equal(RoadError.None, refusal);
        Assert.NotNull(site);
        Assert.Equal(2000.0, site.Length);
        Assert.Equal(2.0, site.Kilometres);
        Assert.False(site.IsBuilt(T));
        Assert.Equal(0.0, site.Progress(T));

        // The bill scales by the kilometre.
        var gravel = T.ResourceIndex("Gravel");
        var perKm = T.Grades[gravelRoad].Materials.First(b => b.Resource == gravel).Tonnes;
        Assert.Equal(perKm * 2.0, site.Wants(gravel, T));

        site.WorkDone = site.Labour(T);
        Assert.True(site.IsBuilt(T));
        Assert.Equal(1.0, site.Progress(T));
    }

    /// <summary>
    /// Without the water check a gravel road could be laid straight across a
    /// river at the price of gravel. Before it existed, nothing asked.
    /// </summary>
    [Fact]
    public void A_road_is_not_a_bridge()
    {
        var works = new RoadWorks(T);
        var terrain = Terrain.Flat(6000.0, 10.0);

        // A river straight across the run.
        for (var y = 0.0; y < 6000.0; y += 10.0)
        {
            terrain.SetSurfaceAt(2000.0, y, Surface.Water);
        }

        Assert.Equal(
            RoadError.NeedsABridge,
            works.Order(1000.0, 1000.0, 3000.0, 1000.0, GradeIndex("Gravel"), false, 0, terrain, out _));

        // A bridge is exactly the thing that may cross it.
        Assert.Equal(
            RoadError.None,
            works.Order(1000.0, 1000.0, 3000.0, 1000.0, GradeIndex("Bridge"), false, 0, terrain, out _));
    }

    /// <summary>
    /// Lamps are a variant of the road rather than something you place — nobody
    /// wants to site four hundred lamp posts — and they are refused on anything
    /// but paved road.
    /// </summary>
    [Fact]
    public void Only_paved_road_carries_lamps()
    {
        var works = new RoadWorks(T);
        var terrain = Terrain.Flat(6000.0, 10.0);

        Assert.Equal(
            RoadError.NoLampsOnThisGrade,
            works.Order(1000.0, 1000.0, 3000.0, 1000.0, GradeIndex("Dirt"), true, 0, terrain, out _));

        var paved = GradeIndex("Paved");
        Assert.True(T.Grades[paved].Lamps);
        Assert.Equal(
            RoadError.None,
            works.Order(1000.0, 1000.0, 3000.0, 1000.0, paved, true, 0, terrain, out var lit));
        Assert.NotNull(lit);

        // A lit road costs more work and more materials than an unlit one.
        works.Order(1000.0, 2000.0, 3000.0, 2000.0, paved, false, 0, terrain, out var dark);
        Assert.NotNull(dark);
        Assert.True(lit.Labour(T) > dark.Labour(T));

        var steel = T.ResourceIndex("Steel");
        Assert.True(lit.Wants(steel, T) >= dark.Wants(steel, T));
    }

    [Fact]
    public void A_run_too_short_to_matter_is_refused()
    {
        var works = new RoadWorks(T);
        var terrain = Terrain.Flat(6000.0, 10.0);

        Assert.Equal(
            RoadError.TooShort,
            works.Order(1000.0, 1000.0, 1001.0, 1000.0, GradeIndex("Gravel"), false, 0, terrain, out _));

        var lines = new LineWorks(T);
        Assert.Equal(
            LineError.TooShort,
            lines.Order(UtilityIndex("Power"), 0.0, 0.0, 1.0, 0.0, 0, out _));
    }

    /// <summary>
    /// A line site becomes a line: it stops being something to deliver to and
    /// starts being something that carries.
    /// </summary>
    [Fact]
    public void A_line_site_becomes_a_line()
    {
        var works = new LineWorks(T);
        var power = UtilityIndex("Power");

        Assert.Equal(LineError.None, works.Order(power, 0.0, 0.0, 4000.0, 0.0, 0, out var site));
        Assert.NotNull(site);
        Assert.Single(works.Sites);
        Assert.Empty(works.Lines);
        Assert.Equal(4.0, site.Kilometres);
        Assert.True(site.Labour(T) > 0.0);

        var line = works.Finish(site);

        Assert.Empty(works.Sites);
        Assert.Single(works.Lines);
        Assert.Equal(4000.0, line.Length);
        Assert.Equal(power, line.Kind);
        Assert.Single(works.OfKind(power));
        Assert.Equal(4000.0, works.TotalLength(power));
    }

    /// <summary>
    /// <b>Loss by the kilometre is what makes where you put the power station
    /// matter.</b> Without it a plant anywhere lights everything, which is the
    /// abstraction lines exist to replace.
    /// </summary>
    [Fact]
    public void A_long_line_delivers_less_than_a_short_one()
    {
        var works = new LineWorks(T);
        var power = UtilityIndex("Power");

        works.Order(power, 0.0, 0.0, 500.0, 0.0, 0, out var shortSite);
        works.Order(power, 0.0, 1000.0, 20_000.0, 1000.0, 0, out var longSite);
        Assert.NotNull(shortSite);
        Assert.NotNull(longSite);

        var near = works.Finish(shortSite);
        var far = works.Finish(longSite);

        Assert.True(near.Efficiency(T) > far.Efficiency(T));
        Assert.InRange(near.Efficiency(T), 0.0, 1.0);
        Assert.InRange(far.Efficiency(T), 0.0, 1.0);

        // A line long enough loses everything rather than going negative.
        works.Order(power, 0.0, 2000.0, 5_000_000.0, 2000.0, 0, out var absurd);
        Assert.NotNull(absurd);
        Assert.Equal(0.0, works.Finish(absurd).Efficiency(T));
    }

    /// <summary>Stock follows the site, and the table stays in step as sites finish.</summary>
    [Fact]
    public void Site_stock_stays_with_its_site()
    {
        var works = new RoadWorks(T);
        var terrain = Terrain.Flat(6000.0, 10.0);
        var gravel = T.ResourceIndex("Gravel");
        var grade = GradeIndex("Gravel");

        works.Order(0.0, 0.0, 1000.0, 0.0, grade, false, 0, terrain, out var first);
        works.Order(0.0, 500.0, 1000.0, 500.0, grade, false, 0, terrain, out var second);
        works.Order(0.0, 900.0, 1000.0, 900.0, grade, false, 0, terrain, out var third);
        Assert.NotNull(first);
        Assert.NotNull(second);
        Assert.NotNull(third);

        works.Stock.Add(works.IndexOf(first.Id), gravel, 5.0);
        works.Stock.Add(works.IndexOf(third.Id), gravel, 9.0);

        works.Finish(second);

        Assert.Equal(2, works.Sites.Count);
        Assert.Equal(5.0, works.Stock.Get(works.IndexOf(first.Id), gravel));
        Assert.Equal(9.0, works.Stock.Get(works.IndexOf(third.Id), gravel));
    }
}

/// <summary>Crews, settlers and visitors — the people who are not residents yet.</summary>
public sealed class PeopleTests
{
    private static Tables T => Fixtures.Tables;

    /// <summary>
    /// A crew is physical: it stands somewhere and has to be fetched back. A
    /// gang with no site, no bus and nowhere to be is stranded, and that is the
    /// state a bus is sent for.
    /// </summary>
    [Fact]
    public void A_gang_with_nowhere_to_be_is_stranded()
    {
        var crews = new Crews();
        var party = crews.Send(office: 5, heads: 8, x: 100.0, y: 100.0);

        Assert.Equal(8, crews.Posted(5));
        Assert.True(party.IsStranded);
        Assert.Single(crews.Stranded());

        party.Working = Destination.RoadSite(1);
        Assert.False(party.IsStranded);
        Assert.Empty(crews.Stranded());
        Assert.Equal(party, crews.WorkingAt(Destination.RoadSite(1)));
        Assert.Null(crews.WorkingAt(Destination.RoadSite(2)));

        // Called off the site, they are standing about again.
        party.Working = null;
        Assert.Single(crews.Stranded());
    }

    /// <summary>
    /// Foreign builders arrive at a frontier post, not in the yard — a gang
    /// standing at the border needing a lift.
    /// </summary>
    [Fact]
    public void Foreign_builders_arrive_at_the_border()
    {
        var crews = new Crews();
        var party = crews.Send(office: 5, heads: 10, x: 0.0, y: 0.0, from: Market.West);

        // On their way in, they are not stranded — they are expected.
        Assert.False(party.IsStranded);
        Assert.Equal(Market.West, party.HiredFrom);

        crews.Hire(office: 5, from: Market.West, heads: 10);
        Assert.Equal(10, crews.HiredAt(5));
        Assert.Equal(10, crews.HiredFromBloc(Market.West));
        Assert.Equal(0, crews.HiredFromBloc(Market.East));
        Assert.Equal(10, crews.HiredTotal());

        // They cost a wage every day, in that bloc's own money.
        Assert.Equal(10 * T.ForeignWage, crews.DailyWage(Market.West, T));
        Assert.Equal(0.0, crews.DailyWage(Market.East, T));

        // Once they reach the office they are simply the office's crew.
        party.HiredFrom = null;
        Assert.True(party.IsStranded);
    }

    /// <summary>
    /// Settlers wait at a post and give up. A republic that advertises for
    /// people and cannot fetch them loses them, which is what makes a coach and
    /// a road to the post part of the cost of growing.
    /// </summary>
    [Fact]
    public void Settlers_waiting_too_long_go_home()
    {
        var migration = new Migration();
        var group = migration.Arrive(0.0, 0.0, 24, today: 100);

        Assert.Equal(24, migration.HeadsWaiting());
        Assert.False(group.HasGivenUp(100 + T.PatienceDays - 1, T));
        Assert.Empty(migration.GiveUp(100 + T.PatienceDays - 1, T));

        // Aboard a coach, they are no longer waiting — patience stops running.
        group.Riding = 7;
        Assert.False(group.HasGivenUp(100 + T.PatienceDays + 500, T));

        group.Riding = null;
        Assert.True(group.HasGivenUp(100 + T.PatienceDays, T));
        Assert.Single(migration.GiveUp(100 + T.PatienceDays, T));
        Assert.Empty(migration.Waiting);
        Assert.Equal(0, migration.HeadsWaiting());
    }

    /// <summary>
    /// A tourist is not a resident. They occupy a bed, spend foreign money and
    /// go home — a hotel with residents would attract settlers to live in it and
    /// be marked down for having no school.
    /// </summary>
    [Fact]
    public void A_visitor_stays_a_fortnight_and_spends_hard_currency()
    {
        var tourism = new Tourism();
        var visit = tourism.Arrive(0.0, 0.0, 20, Market.West, today: 10, T);

        Assert.Equal(10 + T.StayDays, visit.Until);
        Assert.Equal(20 * T.SpendPerHeadPerDay, visit.SpendPerDay(T));
        Assert.Equal(Market.West, visit.Market);

        // Not staying anywhere until a coach has taken them to a hotel.
        Assert.Equal(0, tourism.HeadsStaying());
        visit.StayingAt = 42;
        Assert.Equal(20, tourism.HeadsStaying());

        Assert.False(visit.IsOver(visit.Until - 1));
        Assert.Empty(tourism.Departing(visit.Until - 1));
        Assert.Single(tourism.Departing(visit.Until));

        tourism.Leave(visit);
        Assert.Empty(tourism.Visits);
    }

    /// <summary>
    /// A site set to nothing while the default is set has been opted <i>out</i>;
    /// a site with no instruction at all follows the default. The two are
    /// different states, which is why the override is stored rather than
    /// inferred from a missing entry.
    /// </summary>
    [Fact]
    public void Opting_out_is_not_the_same_as_saying_nothing()
    {
        var policy = new BuildPolicy();
        var site = Destination.Building(7);
        var other = Destination.Building(8);

        policy.SetGlobal(3);
        Assert.Equal(3, policy.CrossingFor(site));
        Assert.False(policy.IsOverridden(site));

        // Opted out: imports nothing, even though the republic has a default.
        policy.SetSite(site, null);
        Assert.Null(policy.CrossingFor(site));
        Assert.True(policy.IsOverridden(site));
        Assert.Equal(1, policy.Overrides);
        Assert.Equal(3, policy.CrossingFor(other));

        // Its own post beats the default.
        policy.SetSite(site, 1);
        Assert.Equal(1, policy.CrossingFor(site));

        policy.ClearSite(site);
        Assert.Equal(3, policy.CrossingFor(site));
        Assert.Equal(0, policy.Overrides);
    }

    /// <summary>
    /// A site cannot be imported for twice over: the allowance falls as goods
    /// are bought in.
    /// </summary>
    [Fact]
    public void A_site_cannot_be_bought_for_twice()
    {
        var policy = new BuildPolicy();
        var site = Destination.RoadSite(2);
        var gravel = T.ResourceIndex("Gravel");

        Assert.Equal(40.0, policy.Allowance(site, gravel, 40.0));

        policy.RecordPurchase(site, gravel, 15.0);
        Assert.Equal(15.0, policy.BoughtFor(site, gravel));
        Assert.Equal(25.0, policy.Allowance(site, gravel, 40.0));

        policy.RecordPurchase(site, gravel, 100.0);
        Assert.Equal(0.0, policy.Allowance(site, gravel, 40.0));

        // A site pulled down leaves nothing behind: ids are never reused, so a
        // stale entry would sit in every save for ever.
        policy.Forget(site);
        Assert.Equal(0.0, policy.BoughtFor(site, gravel));
        Assert.Equal(40.0, policy.Allowance(site, gravel, 40.0));
    }
}
