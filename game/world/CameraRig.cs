using System;
using Godot;

namespace RedRepublic.World;

/// <summary>
/// The camera: free orbit with constrained tilt.
/// </summary>
/// <remarks>
/// <para>
/// Real-time is the thesis, so the view has to be worth watching a lorry cross a
/// field at. That rules out a locked isometric angle, and it rules out a
/// top-down one where a moving vehicle is a dot. What it does not rule out is
/// stopping the player putting the camera underground or looking at the horizon
/// from inside a hill, which is what the tilt clamp is for.
/// </para>
/// <para>
/// The rig is the pivot and the camera hangs off it at a boom length. Orbiting
/// rotates the rig, panning slides it across the ground plane, and zoom changes
/// the boom — which keeps "where am I looking" and "how far away am I" separate,
/// and is what makes the controls feel attached to the ground.
/// </para>
/// </remarks>
public partial class CameraRig : Node3D
{
    /// <summary>How far the camera can tilt from horizontal. Never flat, never straight down.</summary>
    private const float MinPitch = 12.0f;

    private const float MaxPitch = 82.0f;

    private const float MinDistance = 40.0f;
    private const float ZoomStep = 1.15f;

    /// <summary>
    /// Metres a second at ground level, scaled by how high the camera is:
    /// panning at 20 m of altitude and at 4 km should both feel like moving the
    /// map, not like moving the camera.
    /// </summary>
    private const float PanSpeed = 0.55f;

    private const float OrbitSpeed = 0.006f;

    private Camera3D _camera = null!;
    private float _distance = 800.0f;
    private float _pitch = Mathf.DegToRad(50.0f);
    private float _yaw;
    private float _maxDistance = 12_000.0f;
    private float _extent = 6_000.0f;
    private bool _orbiting;

    /// <summary>
    /// Emitted whenever the boom length changes.
    /// </summary>
    /// <remarks>
    /// The sun listens, because Godot's directional shadow range is measured
    /// from the camera and is not something the light can work out for itself.
    /// It defaults to 100 m and this camera opens above a kilometre out, so
    /// without this nothing in frame is ever inside the shadow range and not one
    /// shadow is drawn — with no error and every number healthy.
    /// </remarks>
    [Signal]
    public delegate void DistanceChangedEventHandler(float metres);

    /// <summary>How long the boom is. The sun needs it to size its shadow range.</summary>
    public float Distance => _distance;

    public override void _Ready() => _camera = GetNode<Camera3D>("Camera");

    /// <summary>
    /// Point the camera at the republic, not at the middle of the map.
    /// </summary>
    /// <remarks>
    /// Where a posting is founded is decided by the geology — the shallowest coal
    /// body — so it is routinely nowhere near the centre. Framing the map means
    /// opening on empty ground with the town off the edge of interest.
    /// </remarks>
    public void FrameMap(float extent, float atX, float atZ)
    {
        _extent = extent;
        _maxDistance = extent * 1.6f;
        Position = new Vector3(atX, 0.0f, atZ);

        // Close enough that the buildings read as buildings rather than as marks.
        _distance = Mathf.Max(300.0f, extent * 0.18f);
        Apply();
    }

    /// <summary>
    /// Tilt the camera, in degrees above horizontal. Capture runs use this.
    /// </summary>
    /// <remarks>
    /// The sky is only visible below about 25 degrees. With the pitch fixed at
    /// fifty and changeable only by dragging a mouse, a capture cannot photograph
    /// the sky at all — and a sky shader was once written, rendered and
    /// pronounced broken on the evidence of frames that contained none of it.
    /// </remarks>
    public void SetPitch(float degrees)
    {
        _pitch = Mathf.Clamp(
            Mathf.DegToRad(degrees), Mathf.DegToRad(MinPitch), Mathf.DegToRad(MaxPitch));
        Apply();
    }

    /// <summary>
    /// Put the camera at a chosen boom length, so a look can be judged at the
    /// distance a player actually watches a lorry from rather than from the
    /// altitude that frames a whole posting.
    /// </summary>
    public void SetDistance(float metres)
    {
        _distance = Mathf.Clamp(metres, MinDistance, _maxDistance);
        Apply();
    }

    public override void _Process(double delta)
    {
        var move = Vector3.Zero;
        if (Input.IsKeyPressed(Key.W))
        {
            move.Z -= 1.0f;
        }

        if (Input.IsKeyPressed(Key.S))
        {
            move.Z += 1.0f;
        }

        if (Input.IsKeyPressed(Key.A))
        {
            move.X -= 1.0f;
        }

        if (Input.IsKeyPressed(Key.D))
        {
            move.X += 1.0f;
        }

        if (move == Vector3.Zero)
        {
            return;
        }

        // Normalised so a diagonal is not faster than a straight line, and
        // rotated by the yaw so "forward" means forward on screen.
        move = move.Normalized().Rotated(Vector3.Up, _yaw);
        Position += move * PanSpeed * _distance * (float)delta;
        ClampToMap();
        Apply();
    }

    public override void _UnhandledInput(InputEvent @event)
    {
        if (@event is InputEventMouseButton button)
        {
            switch (button.ButtonIndex)
            {
                case MouseButton.Middle or MouseButton.Right:
                    _orbiting = button.Pressed;
                    break;

                case MouseButton.WheelUp when button.Pressed:
                    _distance = Mathf.Max(MinDistance, _distance / ZoomStep);
                    Apply();
                    break;

                case MouseButton.WheelDown when button.Pressed:
                    _distance = Mathf.Min(_maxDistance, _distance * ZoomStep);
                    Apply();
                    break;

                default:
                    break;
            }
        }
        else if (@event is InputEventMouseMotion motion && _orbiting)
        {
            _yaw -= motion.Relative.X * OrbitSpeed;
            _pitch = Mathf.Clamp(
                _pitch + (motion.Relative.Y * OrbitSpeed),
                Mathf.DegToRad(MinPitch), Mathf.DegToRad(MaxPitch));
            Apply();
        }
    }

    private void ClampToMap()
    {
        // Half a map's worth of slack outside the border, so you can look in at
        // the frontier from beyond it without being able to lose the republic.
        var slack = _extent * 0.5f;
        Position = new Vector3(
            Mathf.Clamp(Position.X, -slack, _extent + slack),
            Position.Y,
            Mathf.Clamp(Position.Z, -slack, _extent + slack));
    }

    private void Apply()
    {
        Rotation = new Vector3(0.0f, _yaw, 0.0f);

        // The camera hangs off the rig, so its position and orientation are both
        // <b>local</b>. Aiming it with a global look-at points it at world zero,
        // which is the map's corner — and the republic then sits off in the
        // bottom-right of the frame with half the screen empty. A Godot camera
        // looks down its own -Z, so pitching it directly is both correct and
        // simpler.
        _camera.Position = new Vector3(0.0f, Mathf.Sin(_pitch), Mathf.Cos(_pitch)) * _distance;
        _camera.Rotation = new Vector3(-_pitch, 0.0f, 0.0f);

        // Every path that moves the boom comes through here, which is why the
        // signal is emitted from one place rather than from the four callers that
        // change the distance. One of them would eventually be added without it.
        EmitSignal(SignalName.DistanceChanged, _distance);
    }
}
