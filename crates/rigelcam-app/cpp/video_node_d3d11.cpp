// Draws frames on Direct3D 11. The decoders and Qt share one device, so a frame is only copied
// inside the GPU, to a texture this item owns, and goes back to its decoder right away.
#define GST_USE_UNSTABLE_API
#include "video_node.h"

#include <d3d11_4.h>
#include <d3dcompiler.h>
#include <dxgi1_2.h>
#include <gst/d3d11/gstd3d11.h>
#include <gst/gst.h>
#include <wrl/client.h>

#include <QtGui/QMatrix4x4>
#include <QtQuick/QQuickGraphicsDevice>
#include <QtQuick/QQuickWindow>

#include <cstdio>
#include <cstring>
#include <utility>

using Microsoft::WRL::ComPtr;

namespace {

ID3D11Device *g_device = nullptr;
ID3D11DeviceContext *g_context = nullptr;
GstD3D11Device *g_gstDevice = nullptr;

const char *kShader = R"(
cbuffer Constants : register(b0) {
    float4x4 mvp;
    float2 size;
    float2 uvOrigin;
    float2 uvScale;
    float gamma;
    float fullRange;
    float3 luma;
};
Texture2D y : register(t0);
Texture2D uv : register(t1);
SamplerState s : register(s0);
struct V { float4 p : SV_POSITION; float2 t : TEXCOORD0; };
V vs(uint id : SV_VertexID) {
    V o;
    float2 corner = float2(id & 1, id >> 1);
    o.t = uvOrigin + corner * uvScale;
    o.p = mul(mvp, float4(corner * size, 0, 1));
    return o;
}
float4 ps(V i) : SV_TARGET {
    float Y = y.Sample(s, i.t).r;
    float2 c = uv.Sample(s, i.t).rg - 0.5;
    if (fullRange < 0.5) {
        Y = (Y - 16.0 / 255.0) * 255.0 / 219.0;
        c *= 255.0 / 224.0;
    }
    float r = Y + 2 * (1 - luma.r) * c.y;
    float b = Y + 2 * (1 - luma.b) * c.x;
    float g = (Y - luma.r * r - luma.b * b) / luma.g;
    float3 color = saturate(float3(r, g, b));
    return float4(gamma == 1 ? color : pow(color, 1 / gamma), 1);
}
)";

struct Constants {
    float mvp[16];
    float size[2];
    float uvOrigin[2];
    float uvScale[2];
    float gamma;
    float fullRange;
    float luma[3];
    float padding;
};
static_assert(sizeof(Constants) % 16 == 0, "constant buffers go in blocks of 16 bytes");

struct Pipeline {
    ComPtr<ID3D11VertexShader> vs;
    ComPtr<ID3D11PixelShader> ps;
    ComPtr<ID3D11SamplerState> sampler;
    ComPtr<ID3D11RasterizerState> plain, scissored;
    ComPtr<ID3D11DepthStencilState> noDepth;
    ComPtr<ID3D11BlendState> opaque;
};

ComPtr<ID3DBlob> compile(const char *entry, const char *target) {
    ComPtr<ID3DBlob> code, errors;
    if (FAILED(D3DCompile(kShader, strlen(kShader), nullptr, nullptr, nullptr, entry, target, 0, 0, &code, &errors)))
        fprintf(stderr, "shader error (%s)\n", errors ? static_cast<char *>(errors->GetBufferPointer()) : entry);
    return code;
}

// Shared by all items, which draw on the same thread and device.
Pipeline &pipeline() {
    static Pipeline p;
    if (!p.vs) {
        auto vs = compile("vs", "vs_5_0");
        auto ps = compile("ps", "ps_5_0");
        g_device->CreateVertexShader(vs->GetBufferPointer(), vs->GetBufferSize(), nullptr, &p.vs);
        g_device->CreatePixelShader(ps->GetBufferPointer(), ps->GetBufferSize(), nullptr, &p.ps);
        D3D11_SAMPLER_DESC sampler{};
        sampler.Filter = D3D11_FILTER_MIN_MAG_LINEAR_MIP_POINT;
        sampler.AddressU = sampler.AddressV = sampler.AddressW = D3D11_TEXTURE_ADDRESS_CLAMP;
        sampler.MaxLOD = D3D11_FLOAT32_MAX;
        g_device->CreateSamplerState(&sampler, &p.sampler);
        D3D11_RASTERIZER_DESC raster{};
        raster.FillMode = D3D11_FILL_SOLID;
        raster.CullMode = D3D11_CULL_NONE;
        raster.DepthClipEnable = TRUE;
        g_device->CreateRasterizerState(&raster, &p.plain);
        raster.ScissorEnable = TRUE;
        g_device->CreateRasterizerState(&raster, &p.scissored);
        D3D11_DEPTH_STENCIL_DESC depth{};
        g_device->CreateDepthStencilState(&depth, &p.noDepth);
        D3D11_BLEND_DESC blend{};
        blend.RenderTarget[0].RenderTargetWriteMask = D3D11_COLOR_WRITE_ENABLE_ALL;
        g_device->CreateBlendState(&blend, &p.opaque);
    }
    return p;
}

struct D3d11Resources : VideoResources {
    RigelFrame next{};
    ComPtr<ID3D11Texture2D> texture;
    ComPtr<ID3D11ShaderResourceView> y, uv;
    ComPtr<ID3D11Buffer> constants;
    UINT textureWidth = 0, textureHeight = 0;
    int width = 0, height = 0, fullRange = 0, matrix = 0;

    void clear() override {
        release(next);
        y.Reset();
        uv.Reset();
        texture.Reset();
        textureWidth = textureHeight = 0;
    }
};

class VideoNode : public VideoNodeBase {
public:
    VideoNode(std::shared_ptr<D3d11Resources> resources, std::shared_ptr<ItemLink> link)
        : m_res(std::move(resources)), m_link(std::move(link)) {}
    ~VideoNode() override { m_res->clear(); }

    void setFrame(const RigelFrame &frame, const QRectF &rect, const QRectF &view, float gamma) override {
        m_rect = rect;
        m_view = view;
        m_gamma = gamma;
        if (frame.release) {
            release(m_res->next);
            m_res->next = frame;
        }
    }

    void requestPhoto(const QString &name) override { m_photoName = name; }

    StateFlags changedStates() const override { return BlendState | ViewportState | ScissorState | DepthState; }
    RenderingFlags flags() const override { return BoundedRectRendering | OpaqueRendering; }
    QRectF rect() const override { return m_rect; }

    // Outside of the render pass, as the copy can't be recorded inside of it on every API, and the
    // frame goes back to the decoder as soon as it was copied.
    void prepare() override {
        if (!m_res->next.release) return;
        copy(m_res->next);
        release(m_res->next);
    }

    void render(const RenderState *state) override {
        if (!m_res->texture) {
            if (!m_photoName.isEmpty()) {
                m_photoName.clear();
                m_link->report({});
            }
            return;
        }
        gst_d3d11_device_lock(g_gstDevice);
        if (!m_photoName.isEmpty()) takePhoto();

        ComPtr<ID3D11RenderTargetView> target;
        g_context->OMGetRenderTargets(1, &target, nullptr);
        ComPtr<ID3D11Resource> resource;
        target->GetResource(&resource);
        ComPtr<ID3D11Texture2D> targetTexture;
        resource.As(&targetTexture);
        D3D11_TEXTURE2D_DESC desc;
        targetTexture->GetDesc(&desc);

        const D3D11_VIEWPORT viewport{0, 0, float(desc.Width), float(desc.Height), 0, 1};
        g_context->RSSetViewports(1, &viewport);
        Pipeline &p = pipeline();
        if (state->scissorEnabled()) {
            // The scene graph counts from the bottom of the target.
            const QRect clip = state->scissorRect();
            const LONG top = LONG(desc.Height) - clip.y() - clip.height();
            const D3D11_RECT rect{clip.x(), top, clip.x() + clip.width(), top + clip.height()};
            g_context->RSSetScissorRects(1, &rect);
            g_context->RSSetState(p.scissored.Get());
        } else {
            g_context->RSSetState(p.plain.Get());
        }
        const QMatrix4x4 mvp = *state->projectionMatrix() * *matrix();
        draw(mvp, float(m_rect.width()), float(m_rect.height()));
        gst_d3d11_device_unlock(g_gstDevice);
    }

private:
    void copy(const RigelFrame &frame) {
        GstBuffer *buffer = gst_sample_get_buffer(static_cast<GstSample *>(frame.sample));
        GstMemory *memory = buffer ? gst_buffer_peek_memory(buffer, 0) : nullptr;
        if (!memory || !gst_is_d3d11_memory(memory)) return;
        GstMapInfo info;
        if (!gst_memory_map(memory, &info, GstMapFlags(GST_MAP_READ | GST_MAP_D3D11))) return;
        auto *source = reinterpret_cast<ID3D11Resource *>(info.data);
        const UINT subresource = gst_d3d11_memory_get_subresource_index(GST_D3D11_MEMORY_CAST(memory));
        ensureTexture(source);
        if (m_res->texture) {
            gst_d3d11_device_lock(g_gstDevice);
            g_context->CopySubresourceRegion(m_res->texture.Get(), 0, 0, 0, 0, source, subresource, nullptr);
            gst_d3d11_device_unlock(g_gstDevice);
            m_res->width = frame.width;
            m_res->height = frame.height;
            m_res->fullRange = frame.full_range;
            m_res->matrix = frame.matrix;
        }
        gst_memory_unmap(memory, &info);
    }

    void ensureTexture(ID3D11Resource *source) {
        ComPtr<ID3D11Texture2D> texture;
        source->QueryInterface(IID_PPV_ARGS(&texture));
        D3D11_TEXTURE2D_DESC desc;
        texture->GetDesc(&desc);
        if (m_res->texture && m_res->textureWidth == desc.Width && m_res->textureHeight == desc.Height) return;
        m_res->y.Reset();
        m_res->uv.Reset();
        m_res->texture.Reset();
        desc.ArraySize = 1;
        desc.MipLevels = 1;
        desc.Usage = D3D11_USAGE_DEFAULT;
        desc.BindFlags = D3D11_BIND_SHADER_RESOURCE;
        desc.MiscFlags = 0;
        desc.CPUAccessFlags = 0;
        if (FAILED(g_device->CreateTexture2D(&desc, nullptr, &m_res->texture))) {
            if (!m_failed) {
                fprintf(stderr, "could not create a %ux%u video texture\n", desc.Width, desc.Height);
                m_link->reportNoPicture();
            }
            m_failed = true;
            return;
        }
        m_res->textureWidth = desc.Width;
        m_res->textureHeight = desc.Height;
        D3D11_SHADER_RESOURCE_VIEW_DESC view{};
        view.ViewDimension = D3D11_SRV_DIMENSION_TEXTURE2D;
        view.Texture2D.MipLevels = 1;
        view.Format = DXGI_FORMAT_R8_UNORM;
        g_device->CreateShaderResourceView(m_res->texture.Get(), &view, &m_res->y);
        view.Format = DXGI_FORMAT_R8G8_UNORM;
        g_device->CreateShaderResourceView(m_res->texture.Get(), &view, &m_res->uv);
    }

    void draw(const QMatrix4x4 &mvp, float width, float height) {
        Pipeline &p = pipeline();
        if (!m_res->constants) {
            D3D11_BUFFER_DESC desc{};
            desc.ByteWidth = sizeof(Constants);
            desc.Usage = D3D11_USAGE_DEFAULT;
            desc.BindFlags = D3D11_BIND_CONSTANT_BUFFER;
            g_device->CreateBuffer(&desc, nullptr, &m_res->constants);
        }
        // Decoders round their surfaces up, so the picture can fill only part of the texture.
        const float scaleX = float(m_res->width) / float(m_res->textureWidth);
        const float scaleY = float(m_res->height) / float(m_res->textureHeight);
        Constants c{};
        memcpy(c.mvp, mvp.constData(), sizeof c.mvp);
        c.size[0] = width;
        c.size[1] = height;
        c.uvOrigin[0] = float(m_view.left()) * scaleX;
        c.uvOrigin[1] = float(m_view.top()) * scaleY;
        c.uvScale[0] = float(m_view.width()) * scaleX;
        c.uvScale[1] = float(m_view.height()) * scaleY;
        c.gamma = m_gamma;
        c.fullRange = float(m_res->fullRange);
        // Weights of red, green and blue, which define BT.601, BT.709 and BT.2020.
        const float weights[3][3] = {{0.299f, 0.587f, 0.114f}, {0.2126f, 0.7152f, 0.0722f}, {0.2627f, 0.678f, 0.0593f}};
        memcpy(c.luma, weights[qBound(0, m_res->matrix, 2)], sizeof c.luma);
        g_context->UpdateSubresource(m_res->constants.Get(), 0, nullptr, &c, 0, 0);

        g_context->OMSetBlendState(p.opaque.Get(), nullptr, 0xffffffff);
        g_context->OMSetDepthStencilState(p.noDepth.Get(), 0);
        g_context->IASetInputLayout(nullptr);
        g_context->IASetPrimitiveTopology(D3D11_PRIMITIVE_TOPOLOGY_TRIANGLESTRIP);
        g_context->VSSetShader(p.vs.Get(), nullptr, 0);
        g_context->PSSetShader(p.ps.Get(), nullptr, 0);
        g_context->VSSetConstantBuffers(0, 1, m_res->constants.GetAddressOf());
        g_context->PSSetConstantBuffers(0, 1, m_res->constants.GetAddressOf());
        ID3D11ShaderResourceView *views[2] = {m_res->y.Get(), m_res->uv.Get()};
        g_context->PSSetShaderResources(0, 2, views);
        g_context->PSSetSamplers(0, 1, p.sampler.GetAddressOf());
        g_context->Draw(4, 0);
        ID3D11ShaderResourceView *none[2] = {nullptr, nullptr};
        g_context->PSSetShaderResources(0, 2, none);
    }

    // Draws the frame at the size of the video, since the item is usually smaller, with the same
    // shader as the screen, and restores the target of the scene graph.
    void takePhoto() {
        const QString name = std::exchange(m_photoName, QString());
        const UINT width = UINT(qMax(1, qRound(m_res->width * m_view.width())));
        const UINT height = UINT(qMax(1, qRound(m_res->height * m_view.height())));
        D3D11_TEXTURE2D_DESC desc{};
        desc.Width = width;
        desc.Height = height;
        desc.MipLevels = 1;
        desc.ArraySize = 1;
        desc.Format = DXGI_FORMAT_R8G8B8A8_UNORM;
        desc.SampleDesc.Count = 1;
        desc.Usage = D3D11_USAGE_DEFAULT;
        desc.BindFlags = D3D11_BIND_RENDER_TARGET;
        ComPtr<ID3D11Texture2D> target, staging;
        ComPtr<ID3D11RenderTargetView> view;
        bool ready = SUCCEEDED(g_device->CreateTexture2D(&desc, nullptr, &target));
        ready = ready && SUCCEEDED(g_device->CreateRenderTargetView(target.Get(), nullptr, &view));
        desc.Usage = D3D11_USAGE_STAGING;
        desc.BindFlags = 0;
        desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ;
        ready = ready && SUCCEEDED(g_device->CreateTexture2D(&desc, nullptr, &staging));
        if (!ready) {
            m_link->report({});
            return;
        }

        ComPtr<ID3D11RenderTargetView> savedTarget;
        ComPtr<ID3D11DepthStencilView> savedDepth;
        g_context->OMGetRenderTargets(1, &savedTarget, &savedDepth);
        UINT viewports = 1;
        D3D11_VIEWPORT savedViewport;
        g_context->RSGetViewports(&viewports, &savedViewport);

        const D3D11_VIEWPORT viewport{0, 0, float(width), float(height), 0, 1};
        g_context->OMSetRenderTargets(1, view.GetAddressOf(), nullptr);
        g_context->RSSetViewports(1, &viewport);
        g_context->RSSetState(pipeline().plain.Get());
        // From the top left of the picture, in the clip space of Direct3D.
        QMatrix4x4 mvp;
        mvp.translate(-1, 1);
        mvp.scale(2.0f / float(width), -2.0f / float(height));
        draw(mvp, float(width), float(height));

        g_context->OMSetRenderTargets(1, savedTarget.GetAddressOf(), savedDepth.Get());
        if (viewports) g_context->RSSetViewports(1, &savedViewport);
        g_context->CopyResource(staging.Get(), target.Get());

        QImage image(int(width), int(height), QImage::Format_RGBX8888);
        D3D11_MAPPED_SUBRESOURCE mapped;
        if (image.isNull() || FAILED(g_context->Map(staging.Get(), 0, D3D11_MAP_READ, 0, &mapped))) {
            m_link->report({});
            return;
        }
        for (UINT row = 0; row < height; row++)
            memcpy(image.scanLine(int(row)), static_cast<char *>(mapped.pData) + row * mapped.RowPitch, width * 4);
        g_context->Unmap(staging.Get(), 0);
        savePhotoImage(image, name, m_link);
    }

    std::shared_ptr<D3d11Resources> m_res;
    std::shared_ptr<ItemLink> m_link;
    QString m_photoName;
    QRectF m_rect;
    QRectF m_view{0, 0, 1, 1};
    float m_gamma = 1;
    bool m_failed = false;
};

} // namespace

std::shared_ptr<VideoResources> createVideoResources() { return std::make_shared<D3d11Resources>(); }

VideoNodeBase *createVideoNode(std::shared_ptr<VideoResources> resources, std::shared_ptr<ItemLink> link) {
    return new VideoNode(std::static_pointer_cast<D3d11Resources>(std::move(resources)), std::move(link));
}

void prepareGraphics() {
    // Variables set from Rust don't reach the C runtime that Qt reads them from on Windows.
    qputenv("QT_QUICK_CONTROLS_STYLE", "Basic");
    QQuickWindow::setGraphicsApi(QSGRendererInterface::Direct3D11);
    const D3D_FEATURE_LEVEL levels[] = {D3D_FEATURE_LEVEL_11_1, D3D_FEATURE_LEVEL_11_0};
    const HRESULT result = D3D11CreateDevice(nullptr, D3D_DRIVER_TYPE_HARDWARE, nullptr,
                                             D3D11_CREATE_DEVICE_BGRA_SUPPORT | D3D11_CREATE_DEVICE_VIDEO_SUPPORT, levels,
                                             ARRAYSIZE(levels), D3D11_SDK_VERSION, &g_device, nullptr, &g_context);
    if (FAILED(result)) {
        fprintf(stderr, "could not create the Direct3D device (0x%lx)\n", static_cast<unsigned long>(result));
        return;
    }
    // The decoders and Qt use the same context from different threads.
    ComPtr<ID3D10Multithread> multithread;
    if (SUCCEEDED(g_context->QueryInterface(IID_PPV_ARGS(&multithread)))) multithread->SetMultithreadProtected(TRUE);
    g_gstDevice = gst_d3d11_device_new_wrapped(g_device);
}

void prepareWindow(QQuickWindow *window) {
    if (g_device) window->setGraphicsDevice(QQuickGraphicsDevice::fromDeviceAndContext(g_device, g_context));
}

extern "C" GstContext *rigel_decoder_context() { return gst_d3d11_context_new(g_gstDevice); }
