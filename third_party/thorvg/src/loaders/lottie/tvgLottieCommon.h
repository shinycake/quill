/*
 * Copyright (c) 2024 the ThorVG project. All rights reserved.

 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files (the "Software"), to deal
 * in the Software without restriction, including without limitation the rights
 * to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 * copies of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:

 * The above copyright notice and this permission notice shall be included in all
 * copies or substantial portions of the Software.

 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 * AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 * OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
 * SOFTWARE.
 */

#ifndef _TVG_LOTTIE_COMMON_
#define _TVG_LOTTIE_COMMON_

#include "tvgRender.h"
#include "tvgStr.h"

namespace tvg
{

struct PathSet
{
    Point* pts = nullptr;
    PathCommand* cmds = nullptr;
    uint32_t ptsCnt = 0;  //QUILL: 32 bits, for long paths.
    uint32_t cmdsCnt = 0;

    void convert(RenderPath& to)
    {
        to.cmds.data = cmds;
        to.cmds.count = cmdsCnt;
        to.pts.data = pts;
        to.pts.count = ptsCnt;
    }

    void copy(const PathSet& rhs)
    {
        ptsCnt = rhs.ptsCnt;
        cmdsCnt = rhs.cmdsCnt;

        if (rhs.ptsCnt) {
            pts = tvg::malloc<Point>(sizeof(Point) * ptsCnt);
            memcpy(pts, rhs.pts, sizeof(Point) * ptsCnt);
        } else pts = nullptr;

        if (rhs.cmdsCnt) {
            cmds = tvg::malloc<PathCommand>(sizeof(PathCommand) * cmdsCnt);
            memcpy(cmds, rhs.cmds, sizeof(PathCommand) * cmdsCnt);
        } else cmds = nullptr;
    }
};

struct RGB32
{
    int32_t r, g, b;
};

struct ColorStop
{
    Fill::ColorStop* data = nullptr;
    Array<float>* input = nullptr;

    void copy(const ColorStop& rhs, uint32_t cnt)
    {
        if (rhs.data) {
            data = tvg::malloc<Fill::ColorStop>(sizeof(Fill::ColorStop) * cnt);
            memcpy(data, rhs.data, sizeof(Fill::ColorStop) * cnt);
        }
        if (rhs.input) TVGERR("LOTTIE", "Must be populated!");
    }
};

struct TextDocument
{
    //QUILL: every field here is optional in the file.
    char* text = nullptr;
    float height = 0.0f;
    float shift = 0.0f;
    RGB32 color{};
    struct {
        Point pos{};
        Point size{};
    } bbox;
    struct {
        RGB32 color{};
        float width = 0.0f;
        bool below = false;
    } stroke;
    char* name = nullptr;
    float size = 0.0f;
    float tracking = 0.0f;
    float justify = 0.0f;    //horizontal alignment
    uint8_t caps = 0;        //0: Regular, 1: AllCaps, 2: SmallCaps

    void copy(const TextDocument& rhs)
    {
        text = tvg::duplicate(rhs.text);
        name = tvg::duplicate(rhs.name);
    }
};

struct AssetSrc
{
    union {
        char* data = nullptr;
        char* path;
    };
    char* mimeType = nullptr;
    uint32_t size = 0;
    bool external = false;

    ~AssetSrc()
    {
        release();
    }

    void release()
    {
        tvg::free(data);
        tvg::free(mimeType);
        data = mimeType = nullptr;
        size = 0;
    }
};

static inline RGB32 lerp(const RGB32& s, const RGB32& e, float t)
{
    //QUILL: clamped as a float, since t comes from a file.
    return {
        (int32_t)tvg::clamp(s.r + (e.r - s.r) * t, 0.0f, 255.0f),
        (int32_t)tvg::clamp(s.g + (e.g - s.g) * t, 0.0f, 255.0f),
        (int32_t)tvg::clamp(s.b + (e.b - s.b) * t, 0.0f, 255.0f)
    };
}

}

#endif //_TVG_LOTTIE_COMMON_
