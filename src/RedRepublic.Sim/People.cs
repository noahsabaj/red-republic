namespace RedRepublic.Sim;

/// <summary>
/// A gang of builders drawn off a Construction Office's staff.
/// </summary>
/// <remarks>
/// <para>
/// <b>A crew is physical.</b> It stands somewhere, it rides a bus to get
/// anywhere, and it has to be fetched back. That is why a site whose materials
/// never arrive would otherwise hold its gang for ever, and why recalling one is
/// a command rather than something the simulation decides.
/// </para>
/// <para>
/// Foreign builders arrive at a <b>frontier post</b>, not in the yard: they are a
/// gang standing at the border needing a lift, over the roads the republic has
/// built, exactly as a crew coming off a finished site is.
/// </para>
/// </remarks>
public sealed class Party
{
    internal Party(int id, int office, int heads, double x, double y, Market? from)
    {
        Id = id;
        Office = office;
        Heads = heads;
        X = x;
        Y = y;
        HiredFrom = from;
    }

    public int Id { get; }

    /// <summary>
    /// The office that employs them. They are drawn from its staff and they go
    /// back to it; an office cannot be pulled down while any of its crews are
    /// out, so this is never dangling.
    /// </summary>
    public int Office { get; }

    public int Heads { get; internal set; }

    /// <summary>
    /// Where they are standing. While aboard a bus this is where the bus was
    /// when they got on — the bus is the thing to draw.
    /// </summary>
    public double X { get; internal set; }

    public double Y { get; internal set; }

    /// <summary>The site they are working, if they are working one.</summary>
    public Destination? Working { get; internal set; }

    /// <summary>The bus they are aboard, if they are aboard one.</summary>
    public int? Riding { get; internal set; }

    /// <summary>
    /// The bloc these builders were hired from, if they are foreign labour on
    /// its way in. Cleared once they reach the office — after that they are
    /// simply the office's crew.
    /// </summary>
    public Market? HiredFrom { get; internal set; }

    /// <summary>
    /// Standing somewhere with no site and no bus: the state the collect job
    /// exists to end.
    /// </summary>
    /// <remarks>
    /// <b>Foreign hands at a post are stranded too.</b> This used to exclude
    /// them — <see cref="HiredFrom"/> being set read as "on their way in" — and
    /// nothing in the republic ever set out for them, so twenty builders paid
    /// for on day one stood at the frontier for a decade while the office read
    /// as fully committed to them. Being hired is not being fetched.
    /// </remarks>
    public bool IsStranded => Working is null && Riding is null;
}

/// <summary>Every gang the republic has out, and every foreign hand on the payroll.</summary>
public sealed class Crews
{
    private readonly List<Party> _parties = [];
    private readonly Dictionary<int, int> _hiredByOffice = [];
    private readonly Dictionary<Market, int> _hiredFromBloc = [];

    private int _nextId = 1;

    public IReadOnlyList<Party> All => _parties;

    public Party? Get(int id)
    {
        foreach (var p in _parties)
        {
            if (p.Id == id)
            {
                return p;
            }
        }

        return null;
    }

    /// <summary>Put a gang back exactly as it was, keeping its id.</summary>
    public Party Restore(
        int id, int office, int heads, double x, double y, Market? from,
        Destination? working, int? riding)
    {
        var party = new Party(id, office, heads, x, y, from)
        {
            Working = working,
            Riding = riding,
        };

        _parties.Add(party);
        _nextId = Math.Max(_nextId, id + 1);
        return party;
    }

    /// <summary>Restore the standing count of who was hired from where.</summary>
    public void RestoreHired(int office, Market from, int heads)
    {
        _hiredByOffice[office] = heads;
        _hiredFromBloc[from] = _hiredFromBloc.GetValueOrDefault(from) + heads;
    }

    /// <summary>Which offices hold hired hands, for the save.</summary>
    public IReadOnlyDictionary<int, int> HiredByOffice => _hiredByOffice;

    public Party Send(int office, int heads, double x, double y, Market? from = null)
    {
        var party = new Party(_nextId++, office, heads, x, y, from);
        _parties.Add(party);
        return party;
    }

    public void Disband(Party party)
    {
        ArgumentNullException.ThrowIfNull(party);
        _parties.Remove(party);
    }

    /// <summary>How many hands one office has out.</summary>
    public int Posted(int office)
    {
        var heads = 0;
        foreach (var p in _parties)
        {
            if (p.Office == office)
            {
                heads += p.Heads;
            }
        }

        return heads;
    }

    /// <summary>The gang working a site, if any.</summary>
    public Party? WorkingAt(Destination site)
    {
        foreach (var p in _parties)
        {
            if (p.Working == site)
            {
                return p;
            }
        }

        return null;
    }

    /// <summary>
    /// Call the gang off a site. They down tools where they stand and wait for
    /// their office to send a bus.
    /// </summary>
    /// <remarks>
    /// There is no way to make people appear back at the office — the same rule
    /// that makes construction physical in the first place. A site that finishes
    /// under them releases them here too, in the same transaction that removes
    /// it, because a party still pointed at a site that no longer exists would be
    /// pointed at nothing.
    /// </remarks>
    public bool Release(Destination site, double x, double y)
    {
        var party = WorkingAt(site);
        if (party is null)
        {
            return false;
        }

        party.Working = null;
        party.X = x;
        party.Y = y;
        return true;
    }

    /// <summary>Gangs standing about with nowhere to be — what a bus is sent for.</summary>
    public List<Party> Stranded()
    {
        var lost = new List<Party>();
        foreach (var p in _parties)
        {
            if (p.IsStranded)
            {
                lost.Add(p);
            }
        }

        return lost;
    }

    public List<Party> OfOffice(int office)
    {
        var found = new List<Party>();
        foreach (var p in _parties)
        {
            if (p.Office == office)
            {
                found.Add(p);
            }
        }

        return found;
    }

    /// <summary>
    /// Take on foreign builders for one office. They cost a fee now and a wage
    /// every day thereafter, both in that bloc's own currency — which is the
    /// whole trade against training your own, who cost the republic no money at
    /// all.
    /// </summary>
    public void Hire(int office, Market from, int heads)
    {
        _hiredByOffice.TryGetValue(office, out var already);
        _hiredByOffice[office] = already + heads;
        _hiredFromBloc.TryGetValue(from, out var bloc);
        _hiredFromBloc[from] = bloc + heads;
    }

    public int HiredAt(int office) => _hiredByOffice.GetValueOrDefault(office);

    public int HiredFromBloc(Market from) => _hiredFromBloc.GetValueOrDefault(from);

    public int HiredTotal()
    {
        var total = 0;
        foreach (var heads in _hiredByOffice.Values)
        {
            total += heads;
        }

        return total;
    }

    /// <summary>What a day of foreign labour costs, per bloc.</summary>
    public double DailyWage(Market from, Tables t)
    {
        ArgumentNullException.ThrowIfNull(t);
        return HiredFromBloc(from) * t.ForeignWage;
    }
}

/// <summary>
/// A party of people waiting at a frontier post to be brought in and housed.
/// </summary>
/// <remarks>
/// They wait, and they give up. A republic that advertises for settlers and then
/// cannot fetch them loses them — which is what makes a coach and a road to the
/// post part of the cost of growing.
/// </remarks>
public sealed class Group(int id, double x, double y, int heads, long since)
{
    public int Id { get; } = id;

    public double X { get; internal set; } = x;

    public double Y { get; internal set; } = y;

    public int Heads { get; internal set; } = heads;

    public int? Riding { get; internal set; }

    /// <summary>The day they arrived at the post.</summary>
    public long Since { get; } = since;

    public bool HasGivenUp(long today, Tables t)
    {
        ArgumentNullException.ThrowIfNull(t);
        return Riding is null && today - Since >= t.PatienceDays;
    }
}

/// <summary>Everyone waiting at the frontier to come in.</summary>
public sealed class Migration
{
    private readonly List<Group> _waiting = [];

    private int _nextId = 1;

    public IReadOnlyList<Group> Waiting => _waiting;

    public int HeadsWaiting()
    {
        var heads = 0;
        foreach (var g in _waiting)
        {
            heads += g.Heads;
        }

        return heads;
    }

    /// <summary>Put a waiting party back exactly as it was, keeping its id.</summary>
    public Group Restore(int id, double x, double y, int heads, long since, int? riding)
    {
        var g = new Group(id, x, y, heads, since) { Riding = riding };
        _waiting.Add(g);
        _nextId = Math.Max(_nextId, id + 1);
        return g;
    }

    public Group Arrive(double x, double y, int heads, long today)
    {
        var group = new Group(_nextId++, x, y, heads, today);
        _waiting.Add(group);
        return group;
    }

    public void Remove(Group group) => _waiting.Remove(group);

    public Group? Get(int id)
    {
        foreach (var g in _waiting)
        {
            if (g.Id == id)
            {
                return g;
            }
        }

        return null;
    }

    /// <summary>A party climbs aboard a coach. They are no longer fetchable.</summary>
    public void Board(int group, int vehicle)
    {
        var g = Get(group);
        if (g is not null)
        {
            g.Riding = vehicle;
        }
    }

    /// <summary>Groups that waited long enough and went home.</summary>
    public List<Group> GiveUp(long today, Tables t)
    {
        var gone = new List<Group>();
        foreach (var g in _waiting)
        {
            if (g.HasGivenUp(today, t))
            {
                gone.Add(g);
            }
        }

        foreach (var g in gone)
        {
            _waiting.Remove(g);
        }

        return gone;
    }
}

/// <summary>
/// A party of visitors from abroad: a bed for a fortnight and hard currency
/// spent every day of it.
/// </summary>
/// <remarks>
/// A tourist is <b>not</b> a resident. They occupy a bed, spend foreign money and
/// go home; they are not counted in the census, do not vote with their feet and
/// are not judged by contentment. A hotel with residents would attract settlers
/// to live in it and be marked down for having no school.
/// </remarks>
public sealed class Visit(int id, double x, double y, int heads, Market market, long since, long until)
{
    public int Id { get; } = id;

    public double X { get; internal set; } = x;

    public double Y { get; internal set; } = y;

    public int Heads { get; } = heads;

    /// <summary>The bloc they came from — and whose currency they spend.</summary>
    public Market Market { get; } = market;

    public int? Riding { get; internal set; }

    /// <summary>The hotel they are staying at, once they have reached one.</summary>
    public int? StayingAt { get; internal set; }

    public long Since { get; } = since;

    public long Until { get; } = until;

    public bool IsOver(long today) => today >= Until;

    /// <summary>What this party spends in a day, in its own bloc's money.</summary>
    public double SpendPerDay(Tables t)
    {
        ArgumentNullException.ThrowIfNull(t);
        return Heads * t.SpendPerHeadPerDay;
    }
}

/// <summary>Every party of visitors in the republic or on its way in.</summary>
public sealed class Tourism
{
    private readonly List<Visit> _visits = [];

    private int _nextId = 1;

    public IReadOnlyList<Visit> Visits => _visits;

    public int HeadsStaying()
    {
        var heads = 0;
        foreach (var v in _visits)
        {
            if (v.StayingAt is not null)
            {
                heads += v.Heads;
            }
        }

        return heads;
    }

    /// <summary>Put a visiting party back exactly as it was, keeping its id.</summary>
    public Visit Restore(
        int id, double x, double y, int heads, Market market, long since, long until,
        int? riding, int? stayingAt)
    {
        var v = new Visit(id, x, y, heads, market, since, until)
        {
            Riding = riding,
            StayingAt = stayingAt,
        };

        _visits.Add(v);
        _nextId = Math.Max(_nextId, id + 1);
        return v;
    }

    public Visit Arrive(double x, double y, int heads, Market market, long today, Tables t)
    {
        ArgumentNullException.ThrowIfNull(t);
        var visit = new Visit(_nextId++, x, y, heads, market, today, today + t.StayDays);
        _visits.Add(visit);
        return visit;
    }

    public void Leave(Visit visit) => _visits.Remove(visit);

    public Visit? Get(int id)
    {
        foreach (var v in _visits)
        {
            if (v.Id == id)
            {
                return v;
            }
        }

        return null;
    }

    /// <summary>A party climbs aboard a coach.</summary>
    public void Board(int visit, int vehicle)
    {
        var v = Get(visit);
        if (v is not null)
        {
            v.Riding = vehicle;
        }
    }

    /// <summary>They reach their beds, and start spending.</summary>
    public void CheckIn(int visit, int hotel)
    {
        var v = Get(visit);
        if (v is not null)
        {
            v.Riding = null;
            v.StayingAt = hotel;
        }
    }

    public List<Visit> Departing(long today)
    {
        var going = new List<Visit>();
        foreach (var v in _visits)
        {
            if (v.IsOver(today))
            {
                going.Add(v);
            }
        }

        return going;
    }
}

/// <summary>
/// Where sites buy materials the republic has not made.
/// </summary>
/// <remarks>
/// <para>
/// <b>This shortens no build.</b> It answers where a tonne of brick comes from in
/// a republic with no brickworks; the goods land at the post and your lorries
/// still have to fetch them.
/// </para>
/// <para>
/// A site set to nothing while the default is set has been opted <i>out</i>; a
/// site with no instruction at all follows the default. The two are different
/// states, which is why the override is stored rather than inferred from a
/// missing entry.
/// </para>
/// </remarks>
public sealed class BuildPolicy
{
    /// <summary>Sites with an instruction of their own — the value may be "none".</summary>
    private readonly Dictionary<Destination, int?> _bySite = [];

    private readonly Dictionary<(Destination Site, int Resource), double> _bought = [];

    /// <summary>The republic's default post, or null to import nothing.</summary>
    public int? Global { get; private set; }

    public void SetGlobal(int? crossing) => Global = crossing;

    public void SetSite(Destination site, int? crossing) => _bySite[site] = crossing;

    /// <summary>Every site with an instruction of its own, for the save.</summary>
    public IReadOnlyDictionary<Destination, int?> Sites => _bySite;

    /// <summary>Put a site back under the republic's default.</summary>
    public bool ClearSite(Destination site) => _bySite.Remove(site);

    public bool IsOverridden(Destination site) => _bySite.ContainsKey(site);

    public int Overrides => _bySite.Count;

    /// <summary>Which post this site imports through, if any.</summary>
    public int? CrossingFor(Destination site) =>
        _bySite.TryGetValue(site, out var own) ? own : Global;

    /// <summary>How much of a material has already been bought in for a site.</summary>
    public double BoughtFor(Destination site, int resource) =>
        _bought.GetValueOrDefault((site, resource));

    /// <summary>
    /// How much more may be bought in — the bill less what has already been
    /// bought, so a site cannot be imported for twice over.
    /// </summary>
    public double Allowance(Destination site, int resource, double bill) =>
        Math.Max(0.0, bill - BoughtFor(site, resource));

    public void RecordPurchase(Destination site, int resource, double tonnes) =>
        _bought[(site, resource)] = BoughtFor(site, resource) + tonnes;

    /// <summary>
    /// Forget a site that no longer exists. Ids are never reused, so a stale
    /// entry would be harmless to read and would sit in every save for ever.
    /// </summary>
    public void Forget(Destination site)
    {
        _bySite.Remove(site);
        foreach (var key in _bought.Keys.Where(k => k.Site == site).ToList())
        {
            _bought.Remove(key);
        }
    }
}
