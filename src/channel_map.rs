// 声道映射器模块 — 按照 ITU-R BS.775 标准实现多声道音频的声道映射
//
// ITU-R BS.775 是国际电信联盟推荐的"多声道立体声系统"标准，
// 定义了常见环绕声格式（如 5.1）的声道排布和缩混建议。
// 本模块实现该标准中推荐的声道间增益系数，确保音频下混/上混时
// 音量平衡和声道间的相对响度关系满足广播级要求。

/// 声道映射器，根据 ITU-R BS.775 标准的建议执行交错音频缓存的逐帧声道变换
pub struct ChannelMapper {
    /// 输入声道数（每帧的输入样本数）
    input_channels: u16,
    /// 输出声道数（每帧的输出样本数）
    output_channels: u16,
}

impl ChannelMapper {
    /// 构造一个新的声道映射器
    ///
    /// # 参数
    /// * `input_channels` — 输入音频流的声道数
    /// * `output_channels` — 输出音频流的声道数
    pub fn new(input_channels: u16, output_channels: u16) -> Self {
        // 确保声道数不为 0：0 声道没有实际意义，panic 防止后续除零错误
        assert!(input_channels > 0, "输入声道数必须大于 0");
        assert!(output_channels > 0, "输出声道数必须大于 0");
        Self {
            input_channels,
            output_channels,
        }
    }

    /// 判断是否为"直通"模式：输入/输出声道数相同时无需任何变换
    pub fn is_passthrough(&self) -> bool {
        self.input_channels == self.output_channels
    }

    /// 执行逐帧声道映射
    ///
    /// 输入和输出均为交错（interleaved）格式的 f32 音频样本：
    /// - 每 `input_channels` 个连续的输入样本构成一帧
    /// - 每 `output_channels` 个连续的输出样本构成一帧
    ///
    /// # 参数
    /// * `input` — 输入交错的音频样本切片
    /// * `output` — 输出交错的音频样本可变切片，长度应匹配帧数 × output_channels
    ///
    /// # 映射策略（逐帧）
    ///
    /// - **直通**：直接逐样本拷贝
    /// - **1ch → 2ch**（单声道上混立体声）：左右声道均复制输入
    /// - **2ch → 1ch**（立体声缩混单声道）：左右等权求和
    /// - **6ch → 2ch**（5.1 环绕声缩混立体声）：采用 ITU-R BS.775 推荐的
    ///   增益系数，通道顺序为 L / C / R / Ls / Rs / LFE
    /// - **其他**：通用兜底 —— 逐通道拷贝，多余通道丢弃，不足通道补 0.0
    pub fn map(&self, input: &[f32], output: &mut [f32]) {
        let ich = self.input_channels as usize;
        let och = self.output_channels as usize;

        // 安全检查：避免除零（构造函数已保证 ich/och > 0）
        if ich == 0 || och == 0 {
            return;
        }

        // 计算音频帧数：取输入帧数和输出帧数中的较小值，防止越界
        let input_frames = input.len() / ich;
        let output_frames = output.len() / och;
        let frames = input_frames.min(output_frames);

        // 直通模式：输入/输出声道数相同，逐样本直接拷贝
        if self.is_passthrough() {
            // 每帧拷贝 ich（即 och）个样本
            let copy_len = frames * ich;
            output[..copy_len].copy_from_slice(&input[..copy_len]);
            // 如果输出缓冲区更大，剩余部分填充静音
            for s in output.iter_mut().skip(copy_len) {
                *s = 0.0;
            }
            return;
        }

        // 非直通模式：逐帧处理
        for f in 0..frames {
            // 当前帧在输入和输出缓冲区中的起始偏移量
            let in_off = f * ich;
            let out_off = f * och;

            // 从输入中借用当前帧的切片（不获取所有权）
            let in_frame = &input[in_off..in_off + ich];
            // 从输出中借用当前帧的可变切片
            let out_frame = &mut output[out_off..out_off + och];

            // ---------- 单声道 → 立体声（1ch → 2ch）----------
            // ITU-R BS.775：单声道上混立体声时，左右声道均复制原始信号，
            // 不做增益衰减，保持感知响度与原始单声道一致
            if ich == 1 && och == 2 {
                out_frame[0] = in_frame[0];
                out_frame[1] = in_frame[0];
            }
            // ---------- 立体声 → 单声道（2ch → 1ch）----------
            // ITU-R BS.775：左右声道等权求和，增益系数各 0.5，
            // 保证求和后功率与原始立体声信号相当（避免过大导致削波）
            else if ich == 2 && och == 1 {
                out_frame[0] = 0.5 * in_frame[0] + 0.5 * in_frame[1];
            }
            // ---------- 5.1 环绕声 → 立体声（6ch → 2ch）----------
            // 通道顺序假设：L(0) / C(1) / R(2) / Ls(3) / Rs(4) / LFE(5)
            //
            // 左声道 = 左前 × 0.5 + 中置 × 0.35 + 左环绕 × 0.35
            // 右声道 = 右前 × 0.5 + 中置 × 0.35 + 右环绕 × 0.35
            //
            // LFE（低频效果）通道按 ITU-R BS.775 建议不混入主声道，
            // 因为消费级设备通常自带低频管理（Bass Management）。
            else if ich == 6 && och == 2 {
                out_frame[0] =
                    0.5 * in_frame[0] + 0.35 * in_frame[1] + 0.35 * in_frame[3];
                out_frame[1] =
                    0.5 * in_frame[2] + 0.35 * in_frame[1] + 0.35 * in_frame[4];
            }
            // ---------- 通用兜底映射 ----------
            // 逐通道拷贝：输入和输出中取较小声道数逐通道对应拷贝，
            // 多余输入通道丢弃，不足输出通道填充 0.0
            else {
                // 计算需要拷贝的通道数（取较小值）
                let copy_ch = ich.min(och);
                for ch in 0..copy_ch {
                    out_frame[ch] = in_frame[ch];
                }
                // 输出声道多于输入声道时，多余位置填静音
                for ch in copy_ch..och {
                    out_frame[ch] = 0.0;
                }
            }
        }

        // 如果输出缓冲区比实际处理的帧数大，将剩余部分填静音
        let processed_samples = frames * och;
        for s in output.iter_mut().skip(processed_samples) {
            *s = 0.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试直通模式：2ch → 2ch
    #[test]
    fn test_passthrough_2ch() {
        let mapper = ChannelMapper::new(2, 2);
        assert!(mapper.is_passthrough());

        // 构造 3 帧立体声数据
        let input = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let mut output = vec![0.0; 6];
        mapper.map(&input, &mut output);
        assert_eq!(output, input);
    }

    /// 测试直通模式：1ch → 1ch
    #[test]
    fn test_passthrough_1ch() {
        let mapper = ChannelMapper::new(1, 1);
        assert!(mapper.is_passthrough());

        let input = vec![0.5, -0.3, 0.8];
        let mut output = vec![0.0; 3];
        mapper.map(&input, &mut output);
        assert_eq!(output, input);
    }

    /// 测试单声道上混立体声：1ch → 2ch
    #[test]
    fn test_mono_to_stereo() {
        let mapper = ChannelMapper::new(1, 2);

        // 2 帧单声道 → 2 帧立体声
        let input = vec![0.5, -0.3];
        let mut output = vec![0.0; 4];
        mapper.map(&input, &mut output);

        assert_eq!(output[0], 0.5);
        assert_eq!(output[1], 0.5);
        assert_eq!(output[2], -0.3);
        assert_eq!(output[3], -0.3);
    }

    /// 测试立体声缩混单声道：2ch → 1ch
    #[test]
    fn test_stereo_to_mono() {
        let mapper = ChannelMapper::new(2, 1);

        // 2 帧立体声 → 2 帧单声道
        let input = vec![1.0, 0.6, -0.2, 0.8];
        let mut output = vec![0.0; 2];
        mapper.map(&input, &mut output);

        let expected0 = 0.5 * 1.0 + 0.5 * 0.6; // 0.8
        let expected1 = 0.5 * (-0.2) + 0.5 * 0.8; // 0.3
        assert!((output[0] - expected0).abs() < 1e-6);
        assert!((output[1] - expected1).abs() < 1e-6);
    }

    /// 测试 5.1 环绕声缩混立体声：6ch → 2ch
    #[test]
    fn test_51_to_stereo() {
        let mapper = ChannelMapper::new(6, 2);

        // L=1.0, C=0.5, R=0.8, Ls=0.3, Rs=0.4, LFE=0.2
        let input = vec![1.0, 0.5, 0.8, 0.3, 0.4, 0.2];
        let mut output = vec![0.0; 2];
        mapper.map(&input, &mut output);

        let expected_l = 0.5 * 1.0 + 0.35 * 0.5 + 0.35 * 0.3; // 0.5 + 0.175 + 0.105 = 0.78
        let expected_r = 0.5 * 0.8 + 0.35 * 0.5 + 0.35 * 0.4; // 0.4 + 0.175 + 0.14 = 0.715
        assert!((output[0] - expected_l).abs() < 1e-6);
        assert!((output[1] - expected_r).abs() < 1e-6);
    }

    /// 测试通用兜底映射：3ch → 4ch（多余丢弃，不足补零）
    #[test]
    fn test_generic_fallback_3ch_to_4ch() {
        let mapper = ChannelMapper::new(3, 4);

        let input = vec![1.0, 2.0, 3.0];
        let mut output = vec![0.0; 4];
        mapper.map(&input, &mut output);

        // 前 3 通道直接拷贝，第 4 通道填 0.0
        assert_eq!(output[0], 1.0);
        assert_eq!(output[1], 2.0);
        assert_eq!(output[2], 3.0);
        assert_eq!(output[3], 0.0);
    }

    /// 测试通用兜底映射：4ch → 2ch（多余丢弃）
    #[test]
    fn test_generic_fallback_4ch_to_2ch() {
        let mapper = ChannelMapper::new(4, 2);

        let input = vec![0.1, 0.2, 0.3, 0.4];
        let mut output = vec![0.0; 2];
        mapper.map(&input, &mut output);

        // 只拷贝前 2 通道，后 2 通道丢弃
        assert_eq!(output[0], 0.1);
        assert_eq!(output[1], 0.2);
    }

    /// 测试输入/输出长度不匹配：输入帧数多于输出帧数时只处理输出能容纳的帧数
    #[test]
    fn test_input_longer_than_output() {
        let mapper = ChannelMapper::new(2, 2);

        // 3 帧输入，但只有 2 帧输出
        let input = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let mut output = vec![0.0; 4];
        mapper.map(&input, &mut output);

        assert_eq!(output, vec![1.0, 2.0, 3.0, 4.0]);
    }

    /// 测试输出长度比输入帧数多的情形：多余部分填静音
    #[test]
    fn test_output_longer_than_input() {
        let mapper = ChannelMapper::new(1, 2);

        // 1 帧输入，但分配了 3 帧输出
        let input = vec![0.5];
        let mut output = vec![0.0; 6];
        mapper.map(&input, &mut output);

        // 第 1 帧应被映射
        assert_eq!(output[0], 0.5);
        assert_eq!(output[1], 0.5);
        // 剩余帧填 0.0
        assert_eq!(output[2], 0.0);
        assert_eq!(output[3], 0.0);
        assert_eq!(output[4], 0.0);
        assert_eq!(output[5], 0.0);
    }
}
