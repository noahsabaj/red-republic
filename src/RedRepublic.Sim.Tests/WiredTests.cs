namespace RedRepublic.Sim.Tests;

/// <summary>
/// Facts the table authored, the checksum covered, and nothing read.
/// </summary>
/// <remarks>
/// <b>The worst shape a gap takes.</b> A figure nobody has written down is a
/// missing feature and looks like one; a figure authored, loaded, checksummed
/// and read by nothing looks finished from every angle except the republic's.
/// Every test here names one that was in that state.
/// </remarks>
public sealed class WiredTests
{
    private static Tables T => Fixtures.Tables;

    /// <summary>
    /// <b>A staffed clinic answers a want.</b>
    /// </summary>
    /// <remarks>
    /// Thirteen buildings author what they serve and the contentment pass wrote
    /// literal zeroes for five of the eight wants, so a Polyclinic changed
    /// nothing anywhere. The arithmetic consequence was the wall: with health,
    /// culture, schooling, cleanliness and safety all zero, a perfectly-run
    /// republic reached 0.596 against an immigration threshold of 0.6 — so
    /// nobody could ever arrive, whatever the player did.
    /// </remarks>
    [Fact]
    public void An_institution_in_reach_answers_the_want_it_serves()
    {
        var world = Estate(out var home);
        var before = world.Buildings.ContentmentAt(home);
        Assert.Equal(0.0, before.Health);

        var clinic = Stand(world, "Clinic",
            world.Buildings.XAt(home) + 60.0, world.Buildings.YAt(home));
        Assert.True(clinic >= 0, "the fixture should be able to stand a clinic beside the estate");
        world.Buildings.SetStaff(clinic, T.BWorkers[world.Buildings.KindAt(clinic)]);
        world.Buildings.SetPowered(clinic, true);

        Day(world);

        var after = world.Buildings.ContentmentAt(home);
        Assert.True(after.Health > 0.0, "a staffed clinic next door should answer some of Health");
        Assert.True(after.Overall(T) > before.Overall(T));
    }

    /// <summary>
    /// And an unstaffed one answers nothing, which is what makes staffing it —
    /// and lighting it — the decision rather than putting it up.
    /// </summary>
    [Fact]
    public void An_institution_nobody_works_at_answers_nothing()
    {
        var world = Estate(out var home);
        var clinic = Stand(world, "Clinic",
            world.Buildings.XAt(home) + 60.0, world.Buildings.YAt(home));
        Assert.True(clinic >= 0);
        world.Buildings.SetStaff(clinic, 0);

        Day(world);
        Assert.Equal(0.0, world.Buildings.ContentmentAt(home).Health);
    }

    /// <summary>
    /// <b>Enrolling at a university does not delete somebody from the republic.</b>
    /// </summary>
    /// <remarks>
    /// The studying flag was set and never cleared: past university age the loop
    /// moved on before reaching it, and a graduate was still marked a student
    /// too. Somebody enrolled at seventeen was out of the workforce at forty,
    /// and because arrivals top out at Schooled, every Graduate-gated works in
    /// the republic was unstaffable by construction — putting up a Polytechnic
    /// actively destroyed your labour force.
    /// </remarks>
    [Fact]
    public void Studying_is_a_state_of_today_and_not_a_mark_on_a_life()
    {
        var world = Estate(out var home);
        var student = world.Citizens.Add(
            world.Buildings.IdAt(home), T.UniversityAgeFrom, T.SchoolDays, 1.0, 0.5);

        world.Citizens.SetStudying(student, true);
        Assert.False(world.Citizens.CanWork(student));

        // No university stands, so nobody is enrolled today whatever they were
        // enrolled in yesterday.
        Day(world);

        Assert.False(world.Citizens.StudyingAt(student));
        Assert.True(
            world.Citizens.CanWork(student),
            "somebody with no place to study is available for work");
    }

    /// <summary>
    /// <b>Contentment is not a one-way valve.</b>
    /// </summary>
    /// <remarks>
    /// Loyalty was computed daily, saved with every republic, and read by
    /// nothing, while the table authored both figures that decide when somebody
    /// leaves. A republic could fail its people for a decade and lose nobody.
    /// </remarks>
    [Fact]
    public void People_who_have_had_enough_go()
    {
        var world = Estate(out var home);
        for (var i = 0; i < 60; i++)
        {
            world.Citizens.AddArrival(world.Buildings.IdAt(home), 25 + (i % 20));
        }

        for (var c = 0; c < world.Citizens.Count; c++)
        {
            world.Citizens.SetLoyalty(c, 0.0);
        }

        var before = world.Citizens.Count;
        for (var day = 0; day < 360; day++)
        {
            Day(world);
        }

        Assert.True(
            world.Citizens.Count < before,
            "a republic nobody is loyal to should lose somebody over a year");
    }

    /// <summary>
    /// <b>Where there is no care, there is no floor under a famine.</b>
    /// </summary>
    /// <remarks>
    /// The unserved-health floor applied to everybody everywhere, against
    /// everything including a contentment of zero — so total famine settled
    /// people at 0.55 health and cost about five per cent of the adults a year.
    /// The sharpest consequence in the game was the mildest. It is a <i>care</i>
    /// floor: nobody is kept alive by a doctor they cannot get to.
    /// </remarks>
    [Fact]
    public void A_famine_with_no_clinic_has_nothing_holding_it_up()
    {
        var world = Estate(out var home);
        var starving = world.Citizens.AddArrival(world.Buildings.IdAt(home), 30);
        world.Citizens.SetHealth(starving, 1.0);

        // Nothing on the shelves, nothing serving them.
        for (var day = 0; day < 240; day++)
        {
            Day(world);
        }

        Assert.True(
            world.Citizens.Count == 0 || world.Citizens.HealthAt(0) < T.HealthUnserved,
            "with no care in reach, health should fall past the served floor");
    }

    /// <summary>
    /// A republic with a policy and a customs house buys in what it cannot make.
    /// </summary>
    /// <remarks>
    /// <c>SetImportPolicy</c> was accepted, journalled and saved, and no pass
    /// ever landed a purchase — the worst form of the exposure failure, a
    /// control wired to nothing. <b>It shortens no build</b>: the goods land at
    /// the post and the lorries still have to fetch them.
    /// </remarks>
    [Fact]
    public void A_site_under_an_import_policy_gets_its_bill_bought_in()
    {
        var world = World.Found(new WorldSpec(1961, 3000.0, 0), T);
        var post = world.Frontier.Crossings[0];
        world.Treasury.Add(post.Bloc, 500_000.0);

        var house = Stand(world, "Customs", post.X, post.Y);
        Assert.True(house >= 0, "the fixture should open a customs house on a post");

        var plot = Somewhere(world, "House");
        var site = world.Issue(Command.Place(T.BuildingIndex("House"), plot.X, plot.Y));
        Assert.True(site.Accepted);

        var bricks = T.ResourceIndex("Bricks");
        Assert.Equal(0.0, world.Buildings.Stock.Get(house, bricks));

        // No policy: nothing is bought, because nothing was asked for.
        Day(world);
        Assert.Equal(0.0, world.Buildings.Stock.Get(house, bricks));

        Assert.True(world.Issue(Command.SetImportPolicy(null, post.Id)).Accepted);
        Day(world);

        Assert.True(
            world.Buildings.Stock.Get(house, bricks) > 0.0,
            "bricks the republic cannot fire should arrive at the post");

        // And the bill is bought once, however long the policy stands.
        var landed = world.Buildings.Stock.Get(house, bricks);
        for (var day = 0; day < 10; day++)
        {
            Day(world);
        }

        Assert.Equal(landed, world.Buildings.Stock.Get(house, bricks), 6);
    }

    /// <summary>An estate with people in it and nothing else.</summary>
    private static World Estate(out int home)
    {
        var world = World.Found(new WorldSpec(1961, 2000.0, 0), T);
        var at = Somewhere(world, "Apartment");
        var placed = world.Issue(Command.Place(T.BuildingIndex("Apartment"), at.X, at.Y));
        Assert.True(placed.Accepted);

        home = world.Buildings.IndexOf(placed.Id);
        world.Buildings.AddWork(home, T.BLabour[world.Buildings.KindAt(home)]);
        world.Citizens.AddArrival(placed.Id, 30);
        return world;
    }

    private static (double X, double Y) Somewhere(World world, string id)
    {
        var kind = T.BuildingIndex(id);
        var middle = world.Terrain.Extent / 2.0;
        var at = Scenario.FindSite(world, kind, middle, middle, middle);
        Assert.NotNull(at);
        return at.Value;
    }

    private static int Stand(World world, string id, double x, double y)
    {
        var kind = T.BuildingIndex(id);
        var at = Scenario.FindSite(world, kind, x, y, 600.0);
        if (at is null)
        {
            return -1;
        }

        var placed = world.Issue(Command.Place(kind, at.Value.X, at.Value.Y));
        if (!placed.Accepted)
        {
            return -1;
        }

        var b = world.Buildings.IndexOf(placed.Id);
        world.Buildings.AddWork(b, T.BLabour[kind]);
        return b;
    }

    private static void Day(World world)
    {
        for (var tick = 0; tick < SimClock.TicksPerDay; tick++)
        {
            world.Tick();
        }
    }
}
