namespace RedRepublic.Sim.Tests;

/// <summary>
/// Choosing your land, and the town a test republic is stood up with.
/// </summary>
public sealed class FoundingTests
{
    private static Tables T => Fixtures.Tables;

    private static ShelfFilter Small => new(2_000.0, 0);

    private static World Founded(ulong seed = 1961, int people = 60)
    {
        var world = World.Found(new WorldSpec(seed, 6_000.0, 0), T);
        Scenario.Town(world, people);
        return world;
    }

    [Fact]
    public void A_shelf_offers_distinct_postings_from_one_master_seed()
    {
        var shelf = Shelf.Derive(1961, Small, T);
        Assert.Equal(Shelf.Size, shelf.Candidates.Count);

        var seeds = new HashSet<ulong>();
        for (var i = 0; i < shelf.Candidates.Count; i++)
        {
            Assert.True(seeds.Add(shelf.Candidates[i].Spec.Seed), "two candidates share a seed");

            // Positions are stable, which is what lets a card be re-rendered.
            Assert.Equal(i, shelf.Candidates[i].Index);
        }
    }

    /// <summary>
    /// <b>Filters transform the seeds, they do not replace them.</b> Flipping
    /// from plains to taiga shows what that does to land you were already
    /// considering, rather than dealing you six strangers. Candidate <i>n</i> is
    /// always candidate <i>n</i>.
    /// </summary>
    [Fact]
    public void A_filter_re_derives_the_same_candidates_under_a_different_sky()
    {
        var plains = Shelf.Derive(1961, Small, T);
        var elsewhere = plains.Refilter(Small with { Climate = 1 }, T);

        for (var i = 0; i < plains.Candidates.Count; i++)
        {
            Assert.Equal(plains.Candidates[i].Spec.Seed, elsewhere.Candidates[i].Spec.Seed);
            Assert.NotEqual(
                plains.Candidates[i].Spec.Climate, elsewhere.Candidates[i].Spec.Climate);
        }
    }

    [Fact]
    public void The_same_master_seed_deals_the_same_shelf()
    {
        var a = Shelf.Derive(1961, Small, T);
        var b = Shelf.Derive(1961, Small, T);

        for (var i = 0; i < a.Candidates.Count; i++)
        {
            Assert.Equal(a.Candidates[i].Spec, b.Candidates[i].Spec);
            Assert.Equal(a.Candidates[i].Stats, b.Candidates[i].Stats);
        }

        Assert.NotEqual(a.Candidates[0].Spec.Seed, Shelf.Derive(1962, Small, T).Candidates[0].Spec.Seed);
    }

    /// <summary>
    /// <b>Founding hands over the land the card described</b>, not a fresh world
    /// built from the same spec. The two are supposed to be identical, and the
    /// difference still matters: regenerating would make the card's honesty rest
    /// on determinism holding, which is an assumption nothing was checking.
    /// </summary>
    [Fact]
    public void The_posting_taken_is_the_land_the_card_advertised()
    {
        var shelf = Shelf.Derive(1961, Small, T);
        var world = shelf.Found(2);

        Assert.NotNull(world);
        Assert.Equal(shelf.Candidates[2].Spec, world.Spec);
        Assert.Equal(shelf.Candidates[2].Stats, Shelf.Survey(world));

        Assert.Null(shelf.Found(99));
    }

    /// <summary>
    /// <b>A republic is a blank slate.</b> Nothing is built and nobody lives
    /// here — the land, a rouble balance, and whatever the player does next. That
    /// is what makes the opening a plan rather than an inheritance.
    /// </summary>
    [Fact]
    public void A_real_founding_places_nothing_at_all()
    {
        var world = World.Found(new WorldSpec(1961, 6_000.0, 0), T);
        Scenario.Found(world);

        Assert.Equal(0, world.Buildings.Count);
        Assert.Equal(0, world.Citizens.Count);
        Assert.Equal(Scenario.GrantRoubles(T), world.Treasury.Of(Market.East));

        // Roubles only. Everything Western is out of reach until the republic has
        // exported something for hard currency, and that is the opening move.
        Assert.Equal(0.0, world.Treasury.Of(Market.West));
    }

    /// <summary>
    /// <b>The founding hand can run the founding.</b>
    /// </summary>
    /// <remarks>
    /// This exists because it drifted in the reference build and nobody noticed:
    /// raising the Construction Office's worker count took the founding past the
    /// settlers it is given, and because the customs house is last in the
    /// priority order it went from half-staffed to <i>empty</i> — a republic that
    /// could no longer clear a tonne through its own border, in a change that was
    /// about construction and said nothing about trade.
    /// <para>
    /// The rule is not that a republic must be comfortable. It is that Moscow
    /// does not send a building and withhold the people to run it, and that a
    /// worker count changed somewhere else may not silently switch off half the
    /// game.
    /// </para>
    /// </remarks>
    [Fact]
    public void The_founding_hand_can_staff_itself()
    {
        var world = Founded(people: Scenario.Settlers);

        var jobs = 0;
        var housing = 0;
        for (var b = 0; b < world.Buildings.Count; b++)
        {
            jobs += world.Buildings.Jobs(b);
            housing += T.BResidents[world.Buildings.KindAt(b)];
        }

        Assert.True(
            jobs <= Scenario.Settlers,
            $"the founding offers {jobs} jobs and Moscow sent {Scenario.Settlers} settlers, so "
            + "the tail of Scenario.Town stands idle — and the tail is the customs house. "
            + "Either Settlers rises or the founding builds less.");

        Assert.True(housing >= Scenario.Settlers, $"nowhere for {Scenario.Settlers} settlers to live");

        // And a day later they are actually in the jobs, which is the half the
        // arithmetic cannot see: work has to be <i>reachable</i>, so a founding
        // that counts up correctly can still leave a building unmanned because
        // nobody can walk to it.
        for (var i = 0; i < SimClock.TicksPerDay; i++)
        {
            world.Tick();
        }

        var short_ = new List<string>();
        for (var b = 0; b < world.Buildings.Count; b++)
        {
            var kind = world.Buildings.KindAt(b);
            if (world.Buildings.IsBuilt(b) && world.Buildings.StaffAt(b) < world.Buildings.Jobs(b))
            {
                short_.Add($"{T.BName[kind]} {world.Buildings.StaffAt(b)}/{world.Buildings.Jobs(b)}");
            }
        }

        Assert.True(short_.Count == 0, $"the founding hand could not fill: {string.Join(", ", short_)}");
    }

    [Fact]
    public void A_founded_republic_has_people_with_homes_and_a_mine()
    {
        var world = Founded();

        Assert.Equal(60, world.Citizens.Count);
        for (var c = 0; c < world.Citizens.Count; c++)
        {
            Assert.True(
                world.Buildings.IndexOf(world.Citizens.HomeAt(c)) >= 0,
                "a citizen lives in a building that is not there");
        }
    }

    /// <summary>
    /// <b>Moscow sends a working town, wired.</b> A plant lights only what is
    /// strung to it, so a founding that placed the buildings and no lines would
    /// arrive dark — not a hard start, but half the game switched off.
    /// </summary>
    [Fact]
    public void A_founded_republic_arrives_lit_and_warm()
    {
        var world = World.Found(new WorldSpec(1961, 6_000.0, 0), T);
        var town = Scenario.Town(world, 120);

        Assert.True(town.Plant >= 0, "no power station was founded");
        Assert.True(town.Boiler >= 0, "no boiler house was founded");
        Assert.NotEmpty(town.Substations);
        Assert.True(world.Grid.Count > 0, "the founding arrived with no lines at all");

        Assert.True(
            world.Grid.NetworkOf(world.Buildings.IdAt(town.Plant), T.UtilityIndex("Power")) >= 0,
            "the power station is strung to nothing");

        foreach (var block in town.Housing)
        {
            Assert.True(
                world.Grid.NetworkOf(world.Buildings.IdAt(block), T.UtilityIndex("Heat")) >= 0,
                "a block has no heat main past it");
        }
    }

    [Fact]
    public void A_founded_republic_actually_runs()
    {
        var world = Founded();
        var town = Scenario.Site(world);
        Assert.True(town.X > 0.0 || town.Y > 0.0);

        for (var i = 0; i < SimClock.TicksPerDay * 10; i++)
        {
            world.Tick();
        }

        var employed = 0;
        for (var c = 0; c < world.Citizens.Count; c++)
        {
            if (world.Citizens.WorkplaceAt(c) >= 0)
            {
                employed++;
            }
        }

        Assert.True(employed > 0, "nobody found work in ten days");
    }

    [Fact]
    public void Founding_is_reproducible()
    {
        var a = Founded();
        var b = Founded();

        Assert.Equal(a.Buildings.Count, b.Buildings.Count);
        Assert.Equal(a.Citizens.Count, b.Citizens.Count);
        Assert.Equal(a.Grid.Count, b.Grid.Count);

        for (var i = 0; i < a.Buildings.Count; i++)
        {
            Assert.Equal(a.Buildings.KindAt(i), b.Buildings.KindAt(i));
            Assert.Equal(a.Buildings.XAt(i), b.Buildings.XAt(i));
            Assert.Equal(a.Buildings.YAt(i), b.Buildings.YAt(i));
        }
    }

    [Fact]
    public void Site_search_finds_ground_and_gives_up_honestly()
    {
        var world = Founded();
        var house = T.BuildingIndex("House");

        Assert.NotNull(Scenario.FindSite(world, house, 3_000.0, 3_000.0, 2_000.0));

        // Nowhere near the map, so nowhere to build.
        Assert.Null(Scenario.FindSite(world, house, 50_000.0, 50_000.0, 100.0));
    }
}
